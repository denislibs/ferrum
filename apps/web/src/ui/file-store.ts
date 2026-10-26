// Хранилище файлов для интерфейса (спека 10): интерфейс `FileStore` и его
// реализация поверх IndexedDB. Вся работа с базой — в worker'е ввода-вывода
// (`worker/io.worker.ts`), здесь — только запросы и ответы. Байты файлов и превью
// передаются (transfer): основной поток их не копирует и не читает.
// Следующие реализации того же интерфейса — File System Access и Tauri (этап 6).

import type { FileMeta, FromIo, IoOp, IoResults, JournalMark, Section, ToIo } from '../core/file-list';
import { nameFromFile } from '../core/file-list';
import { mb } from '../core/limits';
import { MAX_COMMAND_BYTES } from '../core/messages';
import { FERRUM_FILE, pickFile } from './files';

export type { FileMeta, JournalMark, Section } from '../core/file-list';

export interface FileStore {
  /** Окно списка раздела (не больше 200 строк) и общее число записей в нём. */
  list(section: Section, query: string, start: number, count: number): Promise<IoResults['list']>;
  get(id: string): Promise<FileMeta | null>;
  /** Новая запись (сразу видна в списке); `bytes` — содержимое импортированного
   * файла (`File` с диска читает worker). */
  create(name: string, bytes?: Blob): Promise<FileMeta>;
  /** Байты файла (`null` — новый файл, ещё не сохранённый) и пачки журнала,
   * записанные после них, по порядку записи; `session` — наибольший сеанс среди
   * пачек файла (`null` — пачек нет): новый сеанс берётся после него. */
  read(id: string): Promise<{ meta: FileMeta; bytes: ArrayBuffer | null; journal: ArrayBuffer[]; session: number | null }>;
  /** Содержимое и превью (буферы передаются и здесь больше не доступны); имя
   * записи не меняется — для него `rename`. `thumb`:
   * `null` — убрать превью, `'keep'` — оставить прежнее. `base` — поколение файла
   * (`FileMeta.version`), на котором основана запись; файл с тех пор записали —
   * отказ `CONFLICT`. `journal` — пачки этого сеанса до `upTo` и прежних сеансов
   * вошли в снимок: стереть их вместе с записью; `null` — журнала ещё нет. */
  write(
    id: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ): Promise<FileMeta>;
  /** Дописать пачку журнала изменений (буфер передаётся). */
  appendJournal(id: string, mark: JournalMark, bytes: ArrayBuffer): Promise<void>;
  /** Байты файла `Blob`'ом («Скачать копию»); `null` — новый файл без байтов. */
  blob(id: string): Promise<Blob | null>;
  rename(id: string, name: string): Promise<FileMeta>;
  /** Отметить открытие (раздел «Недавние»). */
  touch(id: string): Promise<FileMeta>;
  trash(id: string): Promise<void>;
  restore(id: string): Promise<void>;
  /** Удалить навсегда. */
  remove(id: string): Promise<void>;
  emptyTrash(): Promise<void>;
  thumbnail(id: string): Promise<Blob | null>;
  /** Список изменился (здесь или в другой вкладке); возвращает отписку. */
  onChange(listener: () => void): () => void;
  dispose(): void;
}

/** Тестовые хуки worker'а ввода-вывода (только в тестовом режиме). */
export interface FileStoreTestHooks {
  seed(count: number): Promise<void>;
  failWrites(on: boolean): Promise<void>;
  /** Пачек журнала файла в базе. */
  journalCount(id: string): Promise<number>;
}

export class IdbFileStore implements FileStore, FileStoreTestHooks {
  private readonly worker: Worker;
  private seq = 0;
  private readonly pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private readonly listeners = new Set<() => void>();
  private broken: string | null = null;

  constructor(test = false) {
    this.worker = new Worker(new URL('../worker/io.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromIo>) => this.onMessage(e.data);
    // Worker не загрузился или упал: все ожидающие и следующие запросы — с ошибкой.
    this.worker.onerror = (e) => {
      e.preventDefault?.();
      this.fail(e.message || 'worker хранилища не загрузился');
    };
    this.post({ t: 'init', test });
  }

  private post(msg: ToIo, transfer: Transferable[] = []): void {
    this.worker.postMessage(msg, transfer);
  }

  private fail(message: string): void {
    this.broken = message;
    for (const p of this.pending.values()) p.reject(new Error(message));
    this.pending.clear();
  }

  private onMessage(m: FromIo): void {
    if (typeof m !== 'object' || m === null) return;
    if (m.t === 'changed') {
      for (const l of this.listeners) l();
      return;
    }
    if (m.t !== 'res') return;
    const p = this.pending.get(m.id);
    if (!p) return;
    this.pending.delete(m.id);
    if (m.ok) p.resolve(m.value);
    else p.reject(new Error(m.message));
  }

  private call<K extends IoOp['op']>(op: Extract<IoOp, { op: K }>, transfer: Transferable[] = []): Promise<IoResults[K]> {
    if (this.broken) return Promise.reject(new Error(this.broken));
    const id = ++this.seq;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.post({ t: 'req', id, ...op } as ToIo, transfer);
    });
  }

  list(section: Section, query: string, start: number, count: number) {
    return this.call({ op: 'list', section, query, start, count });
  }
  get(file: string) {
    return this.call({ op: 'get', file });
  }
  create(name: string, bytes?: Blob) {
    return this.call({ op: 'create', name, bytes: bytes ?? null });
  }
  read(file: string) {
    return this.call({ op: 'read', file });
  }
  write(
    file: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    return this.call({ op: 'write', file, bytes, thumb, base, journal }, thumb instanceof ArrayBuffer ? [bytes, thumb] : [bytes]);
  }
  async appendJournal(file: string, mark: JournalMark, bytes: ArrayBuffer) {
    await this.call({ op: 'appendJournal', file, session: mark.session, batch: mark.batch, bytes }, [bytes]);
  }
  journalCount(file: string) {
    return this.call({ op: 'journalCount', file });
  }
  blob(file: string) {
    return this.call({ op: 'blob', file });
  }
  rename(file: string, name: string) {
    return this.call({ op: 'rename', file, name });
  }
  touch(file: string) {
    return this.call({ op: 'touch', file });
  }
  async trash(file: string) {
    await this.call({ op: 'trash', file });
  }
  async restore(file: string) {
    await this.call({ op: 'restore', file });
  }
  async remove(file: string) {
    await this.call({ op: 'delete', file });
  }
  async emptyTrash() {
    await this.call({ op: 'emptyTrash' });
  }
  thumbnail(file: string) {
    return this.call({ op: 'thumb', file });
  }
  async seed(count: number) {
    await this.call({ op: 'seed', count });
  }
  async failWrites(on: boolean) {
    await this.call({ op: 'failWrites', on });
  }
  onChange(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  dispose() {
    this.fail('хранилище закрыто');
    this.worker.terminate();
  }
}

/** Буфер `Uint8Array` целиком — для передачи без копии; часть буфера — копия. */
export function ownBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength && bytes.buffer instanceof ArrayBuffer
    ? bytes.buffer
    : (bytes.slice().buffer as ArrayBuffer);
}

/** «Импорт…» и «Открыть…»: выбрать `.ferrum` на диске и положить новой записью в
 * хранилище. Файл уходит worker'у ссылкой (`Blob`), основной поток его не читает.
 * `null` — пользователь отказался или ошибка (уже показана через `toast`). Разбор
 * файла — при открытии, в ядре: испорченный откроется с сообщением или не откроется. */
export async function importFromDisk(files: FileStore, toast: (text: string) => void): Promise<FileMeta | null> {
  try {
    const f = await pickFile(FERRUM_FILE);
    if (!f) return null;
    // Размер — до записи: файл больше предела ядро всё равно не откроет.
    if (f.size > MAX_COMMAND_BYTES.open) {
      toast(`Не удалось открыть файл: больше ${mb(MAX_COMMAND_BYTES.open)} МБ`);
      return null;
    }
    return await files.create(nameFromFile(f.name), f);
  } catch (e) {
    toast(`Не удалось открыть файл: ${e instanceof Error ? e.message : String(e)}`);
    return null;
  }
}
