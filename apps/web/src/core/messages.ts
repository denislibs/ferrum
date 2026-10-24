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

/** Размер в мегабайтах с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
const mb = (n: number) => Math.ceil(n / (1024 * 1024));

/** Текст ошибки, если команда не годится к отправке (байты больше предела, не
 * `Uint8Array` или лежат в общей памяти); `null` — можно слать. Проверку делают
 * обе стороны: команды из основного потока не доверенные. Не объект — тоже
 * `null`: форму целиком проверяет разбор команды в ядре («Неверная команда»). */
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage') return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  if (!(bytes instanceof Uint8Array)) return 'Неверная команда: байты должны быть Uint8Array.';
  if (typeof SharedArrayBuffer === 'function' && bytes.buffer instanceof SharedArrayBuffer) {
    return 'Неверная команда: байты не могут лежать в общей памяти.';
  }
  const max = MAX_COMMAND_BYTES[cmd.type];
  if (bytes.byteLength > max) {
    return `Слишком большой объём данных: ${mb(bytes.byteLength)} МБ, предел для этой команды — ${mb(max)} МБ.`;
  }
  return null;
}

/** Команда к отправке: байты, занимающие часть буфера, копируются (передача
 * буфера целиком утащила бы чужие данные и сделала бы недействительными другие
 * представления на нём). Возвращает команду и список передаваемых буферов либо
 * текст ошибки. */
export function prepareCommand(cmd: Command): { cmd: Command; transfer: Transferable[] } | { error: string } {
  const error = commandSizeError(cmd);
  if (error) return { error };
  if (typeof cmd !== 'object' || cmd === null || !('bytes' in cmd)) return { cmd, transfer: [] };
  let bytes = cmd.bytes as unknown as Uint8Array;
  if (bytes.byteOffset !== 0 || bytes.byteLength !== bytes.buffer.byteLength) bytes = bytes.slice();
  return { cmd: { ...cmd, bytes } as Command, transfer: [bytes.buffer as ArrayBuffer] };
}

/** Виды сообщений worker'у: остальное — неверный конверт. */
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'tick']);

/** Конверт сообщения worker'у: объект известного вида. Содержимое команды
 * проверяет ядро. */
export function isToWorker(m: unknown): m is ToWorker {
  return typeof m === 'object' && m !== null && TO_WORKER_KINDS.has((m as { t?: unknown }).t as string);
}

export type FromWorker =
  | { t: 'started'; waitAsync: boolean }
  | { t: 'events'; events: CoreEvent[] }
  /** Живое состояние в режиме postMessage (в режиме SAB оно в общей памяти). */
  | { t: 'live'; live: Float64Array }
  | { t: 'fatal'; message: string };
