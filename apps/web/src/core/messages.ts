// Сообщения между основным потоком и worker'ом поверх протокола ядра: команды и
// события ядра (`Command`, `CoreEvent`) описаны в Rust и сгенерированы tsify,
// здесь — только конверт.

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';

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
      /** Асинхронный API не умеет свой формат: событие `clipboard` несёт и текст с меткой. */
      clipboardText: boolean;
    }
  | { t: 'cmd'; cmd: Command }
  /** Вставка текста с меткой из буфера обмена: base64 декодирует worker (основной
   * поток только переносит текст), затем — команда `paste`. */
  | { t: 'pasteText'; text: string }
  /** Текст с меткой для байтов буфера обмена (браузер отказал асинхронному API):
   * ответ — `clipboardText`. */
  | { t: 'encodeClipboard'; bytes: Uint8Array }
  /** Кадр в режиме postMessage: записи ввода с прошлого кадра. В режиме SAB — без
   * ввода, только если в worker'е нет `Atomics.waitAsync`. */
  | { t: 'tick'; input: ArrayBuffer | null };

/** Предел размера байтов в командах: больше worker'у не отправляем (память wasm
 * ограничена 4 ГБ, а копия байтов в ней — ещё и вторая). */
export const MAX_COMMAND_BYTES = {
  open: 512 * 1024 * 1024,
  paste: 64 * 1024 * 1024,
  placeImage: 64 * 1024 * 1024,
  pasteImage: 64 * 1024 * 1024,
} as const;

/** Размер в мегабайтах с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
const mb = (n: number) => Math.ceil(n / (1024 * 1024));

/** Текст ошибки, если команда не годится к отправке (байты больше предела, не
 * `Uint8Array` или лежат в общей памяти); `null` — можно слать. Проверку делают
 * обе стороны: команды из основного потока не доверенные. Не объект — тоже
 * `null`: форму целиком проверяет разбор команды в ядре («Неверная команда»). */
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type !== 'open' && cmd.type !== 'paste' && cmd.type !== 'placeImage' && cmd.type !== 'pasteImage') return null;
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

/** Текст ошибки, если текст вставки не годится (не строка или длиннее предела —
 * до декодирования); `null` — можно слать. Проверяют обе стороны. */
export function pasteTextError(text: string): string | null {
  if (typeof text !== 'string') return 'Неверная команда: текст вставки должен быть строкой.';
  if (text.length > MAX_CLIPBOARD_TEXT) return 'Слишком большой объём данных в буфере обмена: предел — 64 МБ.';
  return null;
}

/** Текст ошибки, если байты для `encodeClipboard` не годятся; `null` — можно. */
export function clipboardBytesError(bytes: Uint8Array): string | null {
  if (!(bytes instanceof Uint8Array)) return 'Неверная команда: байты должны быть Uint8Array.';
  if (typeof SharedArrayBuffer === 'function' && bytes.buffer instanceof SharedArrayBuffer) {
    return 'Неверная команда: байты не могут лежать в общей памяти.';
  }
  if (bytes.byteLength > MAX_CLIPBOARD_BYTES) {
    return `Не удалось скопировать: выделение ${mb(bytes.byteLength)} МБ, предел — 64 МБ`;
  }
  return null;
}

/** Дополнения worker'а к событию `clipboard`: вырезание (документ изменился в той же
 * пачке) и, для запасного пути, готовый текст с меткой. */
export interface ClipboardExtra {
  cut?: boolean;
  text?: string;
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
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick']);

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
  | { t: 'fatal'; message: string }
  /** Ответ на `encodeClipboard`; `null` — отказ (ошибка пришла событием). */
  | { t: 'clipboardText'; text: string | null };
