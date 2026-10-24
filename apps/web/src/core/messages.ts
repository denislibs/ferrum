// Сообщения между основным потоком и worker'ом поверх протокола ядра: команды и
// события ядра (`Command`, `CoreEvent`) описаны в Rust и сгенерированы tsify,
// здесь — только конверт.

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';

export type ToWorker =
  | {
      t: 'init';
      canvas: OffscreenCanvas;
      /** Общая память; `null` — страница не изолирована (режим postMessage). */
      sab: SharedArrayBuffer | null;
      platform: string;
      width: number;
      height: number;
      dpr: number;
      /** Принудительный бэкенд (`webgpu`, `webgl`, `cpu`) или автоматический выбор. */
      backend: string | null;
      /** Тесты: атлас изображений GPU из одной страницы этой стороны. */
      atlas: number | null;
    }
  | { t: 'cmd'; cmd: Command }
  /** Кадр в режиме postMessage: записи ввода с прошлого кадра. В режиме SAB — без
   * ввода, только если в worker'е нет `Atomics.waitAsync`. */
  | { t: 'tick'; input: ArrayBuffer | null };

/** Предел размера байтов в командах: больше worker'у не отправляем (память wasm
 * ограничена 4 ГБ, а копия байтов в ней — ещё и вторая). */
export const MAX_COMMAND_BYTES = {
  open: 512 * 1024 * 1024,
  paste: 64 * 1024 * 1024,
  placeImage: 64 * 1024 * 1024,
} as const;

/** Текст ошибки, если команда не годится к отправке (байты больше предела или не
 * `Uint8Array`); `null` — можно слать. Команды из основного потока не доверенные,
 * проверка формы остаётся и в worker'е. */
export function commandSizeError(cmd: Command): string | null {
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  if (!(bytes instanceof Uint8Array)) return 'Неверная команда: байты должны быть Uint8Array.';
  const max = MAX_COMMAND_BYTES[cmd.type];
  if (bytes.byteLength > max) {
    const mb = (n: number) => Math.round(n / (1024 * 1024));
    return `Слишком большой объём данных: ${mb(bytes.byteLength)} МБ, предел для этой команды — ${mb(max)} МБ.`;
  }
  return null;
}

export type FromWorker =
  | { t: 'started'; waitAsync: boolean }
  | { t: 'events'; events: CoreEvent[] }
  /** Живое состояние в режиме postMessage (в режиме SAB оно в общей памяти). */
  | { t: 'live'; live: Float64Array }
  | { t: 'fatal'; message: string };
