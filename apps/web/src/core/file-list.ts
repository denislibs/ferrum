// Хранилище файлов (спека 10): записи метаданных, разделы экрана файлов, окно
// списка и сообщения между основным потоком и worker'ом ввода-вывода
// (`worker/io.worker.ts`). Сортировка, поиск и окна считаются в worker'е — здесь
// чистые функции, общие для него и модульных тестов. Всё, что пересекает границу
// потоков, не доверенное: проверки — `requestError`.

import { MAX_NAME_CHARS, MAX_OPEN_BYTES, tooLarge } from './limits';

/** Запись о файле — без байтов: список читает только метаданные. Время — мс эпохи. */
export interface FileMeta {
  id: string;
  name: string;
  createdAt: number;
  /** Последняя запись содержимого или имени. */
  editedAt: number;
  /** Последнее открытие в редакторе. */
  openedAt: number;
  /** Размер файла `.ferrum` в байтах (0 — новый, ещё не сохранённый). */
  size: number;
  /** В корзине с этого момента; нет — не в корзине. */
  trashedAt?: number;
  /** Версия превью: растёт с каждой записью превью (адреса картинок в интерфейсе по ней). */
  thumbVersion: number;
}

/** Разделы экрана файлов: недавние — по открытию, черновики — по изменению. */
export type Section = 'recent' | 'drafts' | 'trash';
export const SECTIONS: readonly Section[] = ['recent', 'drafts', 'trash'];

/** Строк в одном окне списка не больше этого (виртуальный список просит видимые). */
export const MAX_WINDOW = 200;
/** Пределы того, что приходит в worker: идентификатор, PNG превью, засев тестов. */
export const MAX_ID_BYTES = 64;
export const MAX_THUMB_BYTES = 8 * 1024 * 1024;
export const MAX_SEED = 10_000;
/** Имя нового файла (как у нового документа ядра). */
export const UNTITLED = 'Без названия';

/** Записи раздела, отфильтрованные по имени (без учёта регистра) и упорядоченные:
 * новые сверху; при равном времени — по имени и id, чтобы порядок не прыгал. */
export function sectionList(all: Iterable<FileMeta>, section: Section, query: string): FileMeta[] {
  const q = query.trim().toLocaleLowerCase('ru');
  const key: (m: FileMeta) => number =
    section === 'recent' ? (m) => m.openedAt : section === 'drafts' ? (m) => m.editedAt : (m) => m.trashedAt ?? 0;
  const out: FileMeta[] = [];
  for (const m of all) {
    if ((section === 'trash') !== (m.trashedAt !== undefined)) continue;
    // «Недавние» — только открытые хотя бы раз.
    if (section === 'recent' && m.openedAt <= 0) continue;
    if (q && !m.name.toLocaleLowerCase('ru').includes(q)) continue;
    out.push(m);
  }
  return out.sort((a, b) => key(b) - key(a) || a.name.localeCompare(b.name, 'ru') || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/** Окно списка: не больше `MAX_WINDOW` строк с `start`. */
export function windowOf<T>(list: readonly T[], start: number, count: number): T[] {
  const s = Math.max(0, Math.floor(start));
  return list.slice(s, s + Math.min(MAX_WINDOW, Math.max(0, Math.floor(count))));
}

/** Имя файла из имени на диске: без `.ferrum`, обрезано до предела; пустое — «Без названия». */
export function nameFromFile(fileName: string): string {
  const base = fileName.replace(/\.ferrum$/i, '').trim();
  return cleanName(base);
}

/** Имя для записи: без управляющих символов, обрезано до `MAX_NAME_CHARS`; пустое — «Без названия». */
export function cleanName(name: string): string {
  // eslint-disable-next-line no-control-regex
  const s = [...name.replace(/[\u0000-\u001f\u007f]/g, ' ').trim()].slice(0, MAX_NAME_CHARS).join('');
  return s || UNTITLED;
}

// ——— Сообщения worker'у ввода-вывода ———

export type IoOp =
  | { op: 'list'; section: Section; query: string; start: number; count: number }
  | { op: 'get'; file: string }
  /** `bytes` — содержимое импортированного файла: `File` с диска уходит ссылкой,
   * читает его worker. */
  | { op: 'create'; name: string; bytes: ArrayBuffer | Blob | null }
  | { op: 'read'; file: string }
  | { op: 'write'; file: string; name: string; bytes: ArrayBuffer; thumb: ArrayBuffer | null }
  | { op: 'rename'; file: string; name: string }
  | { op: 'touch'; file: string }
  | { op: 'trash'; file: string }
  | { op: 'restore'; file: string }
  | { op: 'delete'; file: string }
  | { op: 'emptyTrash' }
  | { op: 'thumb'; file: string }
  /** Тесты: засеять `count` пустых записей (только в тестовом режиме). */
  | { op: 'seed'; count: number }
  /** Тесты: следующие записи содержимого отказывают, как при нехватке места. */
  | { op: 'failWrites'; on: boolean };

export type IoRequest = { id: number } & IoOp;

/** Ответы по видам запроса. */
export interface IoResults {
  list: { total: number; rows: FileMeta[] };
  get: FileMeta | null;
  create: FileMeta;
  read: { meta: FileMeta; bytes: ArrayBuffer | null };
  write: FileMeta;
  rename: FileMeta;
  touch: FileMeta;
  trash: null;
  restore: null;
  delete: null;
  emptyTrash: null;
  thumb: Blob | null;
  seed: null;
  failWrites: null;
}

export type ToIo = { t: 'init'; test: boolean } | ({ t: 'req' } & IoRequest);

export type FromIo =
  | { t: 'res'; id: number; ok: true; value: unknown }
  | { t: 'res'; id: number; ok: false; message: string }
  /** Список изменился (здесь или в другой вкладке): экрану файлов — перечитать окно. */
  | { t: 'changed' };

const BAD = 'Неверный запрос к хранилищу файлов';

function isId(v: unknown): v is string {
  return typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= MAX_ID_BYTES;
}

function isName(v: unknown): v is string {
  return typeof v === 'string' && v.length <= 4096;
}

function isCount(v: unknown, max: number): v is number {
  return typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= max;
}

/** Байты не того вида, лежат в общей памяти или больше предела — текст ошибки. */
function bytesError(v: unknown, max: number): string | null {
  const size = v instanceof ArrayBuffer ? v.byteLength : typeof Blob === 'function' && v instanceof Blob ? v.size : -1;
  if (size < 0) return BAD;
  if (size > max) return tooLarge(max, size);
  return null;
}

/** Текст ошибки, если запрос к worker'у не годится; `null` — можно выполнять.
 * Основной поток не доверенный: проверяет и worker. */
export function requestError(r: unknown): string | null {
  if (typeof r !== 'object' || r === null) return BAD;
  const m = r as Record<string, unknown>;
  if (typeof m.id !== 'number' || !Number.isSafeInteger(m.id)) return BAD;
  switch (m.op) {
    case 'list':
      return SECTIONS.includes(m.section as Section) &&
        isName(m.query) &&
        isCount(m.start, Number.MAX_SAFE_INTEGER) &&
        isCount(m.count, MAX_WINDOW)
        ? null
        : BAD;
    case 'get':
    case 'read':
    case 'touch':
    case 'trash':
    case 'restore':
    case 'delete':
    case 'thumb':
      return isId(m.file) ? null : BAD;
    case 'create':
      if (!isName(m.name)) return BAD;
      return m.bytes === null ? null : bytesError(m.bytes, MAX_OPEN_BYTES);
    case 'write':
      if (!isId(m.file) || !isName(m.name)) return BAD;
      if (!(m.bytes instanceof ArrayBuffer) || !(m.thumb === null || m.thumb instanceof ArrayBuffer)) return BAD;
      return bytesError(m.bytes, MAX_OPEN_BYTES) ?? (m.thumb === null ? null : bytesError(m.thumb, MAX_THUMB_BYTES));
    case 'rename':
      return isId(m.file) && isName(m.name) ? null : BAD;
    case 'emptyTrash':
      return null;
    case 'seed':
      return isCount(m.count, MAX_SEED) ? null : BAD;
    case 'failWrites':
      return typeof m.on === 'boolean' ? null : BAD;
    default:
      return BAD;
  }
}
