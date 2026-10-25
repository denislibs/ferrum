// Транспорт между основным потоком и worker'ом. Две реализации, выбор
// автоматический: SharedArrayBuffer + postMessage, если страница изолирована
// (`crossOriginIsolated`), иначе только postMessage. Команды и события всегда идут
// сообщениями; ввод и живое состояние — через общую память или в сообщениях кадра.

import { Ctrl, CTRL_BYTES, Field, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET, SAB_BYTES } from './layout.gen';
import type { Sink } from './input';
import { commandSizeError, pasteTextError, type FromWorker, type ToWorker } from './messages';
import { addSaturating, evictOldestDroppable, isDroppableKind, LiveReader } from './shared';

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
  /** Сколько сообщений worker'а отброшено как неверные (форма, неизвестный вид). */
  malformed(): number;
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

const KNOWN_MESSAGES = new Set(['started', 'events', 'live', 'fatal', 'clipboardText']);

/** Проверка сообщения worker'а: известный вид и, для живого состояния, форма. */
function wellFormed(m: unknown): m is FromWorker {
  if (typeof m !== 'object' || m === null) return false;
  const msg = m as { t?: unknown; live?: unknown };
  if (typeof msg.t !== 'string' || !KNOWN_MESSAGES.has(msg.t)) return false;
  if (msg.t === 'live') return msg.live instanceof Float64Array && msg.live.length === LIVE_SLOTS;
  return true;
}

/** Первый неверный вид сообщения — в консоль, остальные только считаются. */
let warned = false;
function warnMalformed(m: unknown): void {
  if (warned) return;
  warned = true;
  console.warn('ferrum: неверное сообщение worker\'а', m);
}

/** Запись из «черновика» нужного вида можно потерять при переполнении. */
function droppable(scratch: DataView): boolean {
  return isDroppableKind(scratch.getUint8(Field.KIND));
}

export class SabTransport implements Transport {
  readonly mode = 'sab' as const;
  readonly sab = new SharedArrayBuffer(SAB_BYTES);
  private readonly ctrl = new Int32Array(this.sab, 0, CTRL_BYTES / 4);
  private readonly liveView = new Float64Array(this.sab, LIVE_OFFSET, LIVE_SLOTS);
  private readonly ring = new Uint8Array(this.sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE);
  private readonly pendingBytes = new Uint8Array(PENDING_MAX * RECORD_SIZE);
  private readonly scratch = new DataView(new ArrayBuffer(RECORD_SIZE));
  private readonly scratchBytes = new Uint8Array(this.scratch.buffer);
  private pendingCount = 0;
  private bad = 0;
  private readonly reader = new LiveReader(this.ctrl, this.liveView);
  /** Worker без `Atomics.waitAsync` (Firefox) будится сообщением. */
  needsTickMessages = false;
  /** Версия живого состояния при последнем тике-сообщении: пока worker не
   * записал новое состояние, прошлый тик не обработан и новый не шлём. */
  private tickSeq = -1;
  private listener: ((msg: FromWorker) => void) | null = null;

  constructor(private readonly worker: Worker) {
    worker.onmessage = (e: MessageEvent<unknown>) => {
      // В режиме SAB живое состояние идёт через общую память: сообщение `live` лишнее.
      if (!wellFormed(e.data) || e.data.t === 'live') {
        this.bad++;
        warnMalformed(e.data);
        return;
      }
      if (e.data.t === 'started') this.needsTickMessages = !e.data.waitAsync;
      this.listener?.(e.data);
    };
  }

  readonly sink: Sink = (fill) => {
    // Запись собирается в черновике: вид (теряемая или нет) известен только после.
    fill(this.scratch, 0);
    if (this.pendingCount === 0 && this.tryWrite(this.scratchBytes)) return;
    // Кольцо полно (worker занят, например открывает документ): запись ждёт в
    // очереди и уходит следующим кадром; порядок событий сохраняется.
    if (this.pendingCount === PENDING_MAX) {
      // Очередь тоже полна. Движение и колесо теряем; нажатие, отпускание, клавиши,
      // blur и размер не теряем никогда: вместо них уходит самая старая теряемая запись.
      if (droppable(this.scratch) || !evictOldestDroppable(this.pendingBytes, this.pendingCount)) {
        addSaturating(this.ctrl, Ctrl.INPUT_DROPPED, 1);
        return;
      }
      this.pendingCount--;
      addSaturating(this.ctrl, Ctrl.INPUT_DROPPED, 1);
    }
    this.pendingBytes.set(this.scratchBytes, this.pendingCount * RECORD_SIZE);
    this.pendingCount++;
  };

  private tryWrite(record: Uint8Array): boolean {
    const w = Atomics.load(this.ctrl, Ctrl.INPUT_WRITE);
    const r = Atomics.load(this.ctrl, Ctrl.INPUT_READ);
    // Беззнаково: счётчики — int32 по модулю 2^32, разность не должна уходить в минус.
    if (((w - r) >>> 0) >= RING_CAPACITY) return false;
    this.ring.set(record, ((w >>> 0) % RING_CAPACITY) * RECORD_SIZE);
    // Запись видна worker'у не раньше нового значения счётчика.
    Atomics.store(this.ctrl, Ctrl.INPUT_WRITE, (w + 1) | 0);
    return true;
  }

  private flushPending(): void {
    let sent = 0;
    while (sent < this.pendingCount) {
      const from = sent * RECORD_SIZE;
      if (!this.tryWrite(this.pendingBytes.subarray(from, from + RECORD_SIZE))) break;
      sent++;
    }
    if (sent > 0) {
      this.pendingBytes.copyWithin(0, sent * RECORD_SIZE, this.pendingCount * RECORD_SIZE);
      this.pendingCount -= sent;
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

  malformed(): number {
    return this.bad;
  }

  post(msg: ToWorker, transfer: Transferable[] = []): string | null {
    const error = msg.t === 'cmd' ? commandSizeError(msg.cmd) : msg.t === 'pasteText' ? pasteTextError(msg.text) : null;
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
  private bad = 0;
  private readonly scratch = new DataView(new ArrayBuffer(RECORD_SIZE));
  private readonly scratchBytes = new Uint8Array(this.scratch.buffer);
  private listener: ((msg: FromWorker) => void) | null = null;

  constructor(private readonly worker: Worker) {
    worker.onmessage = (e: MessageEvent<unknown>) => {
      const m = e.data;
      if (typeof m === 'object' && m !== null && (m as { t?: unknown }).t === 'live') {
        // Ответ получен в любом случае: иначе неверная форма остановила бы кадры.
        this.inFlight = false;
      }
      if (!wellFormed(m)) {
        this.bad++;
        warnMalformed(m);
        return;
      }
      if (m.t === 'live') {
        this.lastLive = m.live;
        return;
      }
      this.listener?.(m);
    };
  }

  readonly sink: Sink = (fill) => {
    fill(this.scratch, 0);
    if (this.count * RECORD_SIZE === this.buf.byteLength) {
      if (this.count >= PENDING_MAX) {
        // Как в SAB: движение и колесо теряем, остальное вытесняет самое старое из них.
        const bytes = new Uint8Array(this.buf);
        if (droppable(this.scratch) || !evictOldestDroppable(bytes, this.count)) {
          this.dropCount++;
          return;
        }
        this.count--;
        this.dropCount++;
      } else {
        const bigger = new ArrayBuffer(this.buf.byteLength * 2);
        new Uint8Array(bigger).set(new Uint8Array(this.buf));
        this.buf = bigger;
        this.view = new DataView(bigger);
      }
    }
    new Uint8Array(this.buf).set(this.scratchBytes, this.count * RECORD_SIZE);
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

  malformed(): number {
    return this.bad;
  }

  post(msg: ToWorker, transfer: Transferable[] = []): string | null {
    const error = msg.t === 'cmd' ? commandSizeError(msg.cmd) : msg.t === 'pasteText' ? pasteTextError(msg.text) : null;
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
