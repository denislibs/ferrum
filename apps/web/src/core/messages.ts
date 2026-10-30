// Сообщения между основным потоком и worker'ом поверх протокола ядра: команды и
// события ядра (`Command`, `CoreEvent`) описаны в Rust и сгенерированы tsify,
// здесь — только конверт.

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';
import { MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from './clipboard-text';
import { MAX_DATA_BYTES, MAX_FONT_BYTES, MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, mb, tooLarge } from './limits';

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
      /** Тестовый режим страницы: доступны тестовые сообщения (`testPanic`). */
      test: boolean;
    }
  /** Только тестовый режим: настоящая паника в ядре (проверка перезапуска). */
  | { t: 'testPanic' }
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
  open: MAX_OPEN_BYTES,
  /** Файл вместе со всеми пачками журнала. */
  openWithJournal: MAX_OPEN_BYTES,
  paste: MAX_DATA_BYTES,
  placeImage: MAX_DATA_BYTES,
  pasteImage: MAX_DATA_BYTES,
  /** Файл шрифта (`loadFont`; без байтов — файл получить не удалось). */
  loadFont: MAX_FONT_BYTES,
} as const;

/** Байты команды не того вида (не массив байтов) — текст без технических имён. */
const BAD_BYTES = 'Неверная команда: данные не в виде байтов';
const SHARED_BYTES = 'Неверная команда: данные не могут лежать в общей памяти';

/** Текст ошибки, если команда не годится к отправке (байты больше предела, не
 * `Uint8Array` или лежат в общей памяти); `null` — можно слать. Тексты — для
 * пользователя: без точки в конце и без имён типов. Проверку делают
 * обе стороны: команды из основного потока не доверенные. Не объект — тоже
 * `null`: форму целиком проверяет разбор команды в ядре («Неверная команда»). */
export function commandSizeError(cmd: Command): string | null {
  if (typeof cmd !== 'object' || cmd === null) return null;
  if (cmd.type === 'openWithJournal') return journalCommandError(cmd);
  if (cmd.type === 'loadFont' && (cmd as { bytes?: unknown }).bytes === undefined) return null;
  if (
    cmd.type !== 'open' &&
    cmd.type !== 'paste' &&
    cmd.type !== 'placeImage' &&
    cmd.type !== 'pasteImage' &&
    cmd.type !== 'loadFont'
  )
    return null;
  const bytes = (cmd as { bytes: unknown }).bytes;
  const kind = bytesKindError(bytes);
  if (kind) return kind;
  const max = MAX_COMMAND_BYTES[cmd.type];
  const size = (bytes as Uint8Array).byteLength;
  if (size > max) return tooLarge(max, size);
  return null;
}

/** Байты не того вида или в общей памяти — текст ошибки; годятся — `null`. */
function bytesKindError(bytes: unknown): string | null {
  if (!(bytes instanceof Uint8Array)) return BAD_BYTES;
  if (typeof SharedArrayBuffer === 'function' && bytes.buffer instanceof SharedArrayBuffer) return SHARED_BYTES;
  return null;
}

/** Файл с журналом: каждая пачка — байтами, их не больше предела, а файл вместе с
 * журналом — не больше предела открытия. */
function journalCommandError(cmd: { bytes: unknown; journal: unknown }): string | null {
  const own = bytesKindError(cmd.bytes);
  if (own) return own;
  if (!Array.isArray(cmd.journal)) return BAD_BYTES;
  if (cmd.journal.length > MAX_JOURNAL_ENTRIES) return `Неверная команда: в журнале изменений больше ${MAX_JOURNAL_ENTRIES} записей`;
  let total = (cmd.bytes as Uint8Array).byteLength;
  for (const b of cmd.journal) {
    const e = bytesKindError(b);
    if (e) return e;
    total += (b as Uint8Array).byteLength;
  }
  return total > MAX_OPEN_BYTES ? tooLarge(MAX_OPEN_BYTES, total) : null;
}

/** Текст ошибки, если текст вставки не годится (не строка или длиннее предела —
 * до декодирования); `null` — можно слать. Проверяют обе стороны. */
export function pasteTextError(text: string): string | null {
  if (typeof text !== 'string') return 'Неверная команда: вставка не текстом';
  if (text.length > MAX_CLIPBOARD_TEXT) return tooLarge(MAX_CLIPBOARD_BYTES);
  return null;
}

/** Текст ошибки, если байты для `encodeClipboard` не годятся; `null` — можно. */
export function clipboardBytesError(bytes: Uint8Array): string | null {
  if (!(bytes instanceof Uint8Array)) return BAD_BYTES;
  if (typeof SharedArrayBuffer === 'function' && bytes.buffer instanceof SharedArrayBuffer) return SHARED_BYTES;
  if (bytes.byteLength > MAX_CLIPBOARD_BYTES) return copyTooLarge(bytes.byteLength, false);
  return null;
}

/** Выделение больше предела буфера обмена: не скопировано (или вырезано, но не
 * записано — тогда подсказка, как вернуть). */
export function copyTooLarge(bytes: number, cut: boolean): string {
  return cut
    ? `Не удалось вырезать в буфер обмена: выделение ${mb(bytes)} МБ, предел — ${mb(MAX_CLIPBOARD_BYTES)} МБ. Отмена вернёт вырезанные объекты`
    : `Не удалось скопировать: выделение ${mb(bytes)} МБ, предел — ${mb(MAX_CLIPBOARD_BYTES)} МБ`;
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
  if (typeof cmd !== 'object' || cmd === null || !('bytes' in cmd) || cmd.bytes === undefined) return { cmd, transfer: [] };
  const own = (b: Uint8Array) => (b.byteOffset !== 0 || b.byteLength !== b.buffer.byteLength ? b.slice() : b);
  const bytes = own(cmd.bytes as unknown as Uint8Array);
  if (cmd.type === 'openWithJournal') {
    const journal = cmd.journal.map(own);
    // Один буфер в списке передачи дважды — DataCloneError: общий буфер — одной записью.
    const transfer = [...new Set([bytes.buffer, ...journal.map((j) => j.buffer)])] as ArrayBuffer[];
    return { cmd: { ...cmd, bytes, journal }, transfer };
  }
  return { cmd: { ...cmd, bytes } as Command, transfer: [bytes.buffer as ArrayBuffer] };
}

/** Виды сообщений worker'у: остальное — неверный конверт. */
export const TO_WORKER_KINDS: ReadonlySet<string> = new Set(['init', 'cmd', 'pasteText', 'encodeClipboard', 'tick', 'testPanic']);

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
