// Обмен через общую память: вычитывание кольца ввода и «секвенсный замок» живого
// состояния. Вынесено из worker'а и транспорта, чтобы проверять без браузера
// (e2e/shared.spec.ts). Всё, что читается из общей памяти, не доверенное.

import { Ctrl, RECORD_SIZE, RING_CAPACITY } from './layout.gen';

export interface Drained {
  /** Сколько записей положено в `out`. */
  count: number;
  /** Сколько записей пропало: счётчик записи ушёл вперёд больше чем на ёмкость кольца. */
  excess: number;
}

/**
 * Вычитывает записи кольца: всё до счётчика записи основного потока.
 * Порядок важен: сначала `Atomics.load` счётчика записи (он публикует данные),
 * потом чтение байтов, и только после — `Atomics.store` счётчика чтения.
 * Больше ёмкости кольца не читается: старые записи уже перезаписаны, поэтому
 * чтение догоняет запись, а лишнее считается в `Ctrl.INPUT_DROPPED`.
 */
export function drainRing(ctrl: Int32Array, ring: Uint8Array, out: Uint8Array): Drained {
  const w = Atomics.load(ctrl, Ctrl.INPUT_WRITE);
  const r = Atomics.load(ctrl, Ctrl.INPUT_READ);
  // Счётчики — int32 по модулю 2^32; ёмкость кольца делит 2^32, так что индексы
  // не прыгают при переходе через границу.
  const pending = (w - r) >>> 0;
  const count = Math.min(pending, RING_CAPACITY);
  const excess = pending - count;
  let at = (((w - count) >>> 0) % RING_CAPACITY) * RECORD_SIZE;
  for (let i = 0; i < count; i++) {
    out.set(ring.subarray(at, at + RECORD_SIZE), i * RECORD_SIZE);
    at += RECORD_SIZE;
    if (at === RING_CAPACITY * RECORD_SIZE) at = 0;
  }
  if (excess > 0) Atomics.add(ctrl, Ctrl.INPUT_DROPPED, excess);
  Atomics.store(ctrl, Ctrl.INPUT_READ, w);
  return { count, excess };
}

/**
 * Запись живого состояния (worker): версия становится нечётной до слотов и чётной
 * после. Читатель по чётности видит, что запись идёт.
 */
export function writeLive(ctrl: Int32Array, slots: Float64Array, values: ArrayLike<number>): void {
  Atomics.add(ctrl, Ctrl.LIVE_SEQ, 1);
  slots.set(values);
  Atomics.add(ctrl, Ctrl.LIVE_SEQ, 1);
}

/** Сколько раз читатель повторяет чтение, пока идёт запись. */
export const LIVE_READ_RETRIES = 4;

/** Читатель живого состояния (основной поток). Никогда не ждёт: не вышло за
 * `LIVE_READ_RETRIES` попыток — отдаёт прошлые значения. */
export class LiveReader {
  private last: Float64Array | null = null;
  private lastSeq = 0;

  constructor(
    private readonly ctrl: Int32Array,
    private readonly slots: Float64Array,
  ) {}

  read(): Float64Array | null {
    for (let i = 0; i < LIVE_READ_RETRIES; i++) {
      const before = Atomics.load(this.ctrl, Ctrl.LIVE_SEQ);
      if (before === 0) return null;
      // Нечётная версия — worker пишет прямо сейчас.
      if ((before & 1) === 1) continue;
      if (before === this.lastSeq && this.last) return this.last;
      const copy = this.slots.slice();
      // Другая версия после чтения — запись прошла посередине, копия порвана.
      if (Atomics.load(this.ctrl, Ctrl.LIVE_SEQ) === before) {
        this.lastSeq = before;
        this.last = copy;
        return copy;
      }
    }
    return this.last;
  }
}
