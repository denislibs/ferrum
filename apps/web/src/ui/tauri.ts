// Десктоп (Tauri 2, этап 6): хранилище файлов и настройки — в Rust (`apps/desktop`,
// крейт `ferrum-desktop-store`), интерфейс обращается к ним командами IPC. IPC есть
// только у окна, поэтому байты файла, журнала и превью идут в Rust из основного потока
// тем же `ArrayBuffer` сырым телом запроса (без JSON и без копий в JS), а запись,
// `fsync` и переименование — в потоке Tauri. Пути на диске интерфейс не видит и не
// передаёт: их дают только родные диалоги в Rust. Окно одно: изменения списка
// интерфейс узнаёт от своих же вызовов, `BroadcastChannel` не нужен.

import type { FileMeta, IoResults, JournalMark, Section } from '../core/file-list';
import type { DiskFiles } from './disk';
import type { FileStore } from './file-store';
import type { Settings } from './settings';
import type { SettingsBackend } from './settings-store';

type Invoke = (cmd: string, args?: unknown, options?: { headers?: Record<string, string> }) => Promise<unknown>;

function ipc(): Invoke | null {
  if (typeof window === 'undefined') return null;
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { invoke?: Invoke } }).__TAURI_INTERNALS__;
  return typeof internals?.invoke === 'function' ? internals.invoke.bind(internals) : null;
}

/** Окно Tauri: IPC из `window.__TAURI_INTERNALS__` (его же вызывает `@tauri-apps/api/core`). */
export function isTauri(): boolean {
  return ipc() !== null;
}

/** Вызов команды; отказ Rust (строка) — `Error` с тем же текстом. */
async function invoke<T>(cmd: string, args?: unknown, headers?: Record<string, string>): Promise<T> {
  const call = ipc();
  if (!call) throw new Error('Нет связи с приложением');
  try {
    return (await call(cmd, args, headers ? { headers } : undefined)) as T;
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
  }
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID и числа). */
function raw<T>(cmd: string, bytes: ArrayBuffer, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, new Uint8Array(bytes), { 'x-ferrum': JSON.stringify(args) });
}

const bytesOrNull = (b: ArrayBuffer) => (b.byteLength > 0 ? b : null);

export class TauriFileStore implements FileStore, SettingsBackend {
  private readonly listeners = new Set<() => void>();

  /** Файлы на диске — родными диалогами Rust; доступ к файлу есть всегда. */
  readonly disk: DiskFiles = {
    open: () => this.changed(invoke<FileMeta | null>('files_open_dialog')),
    saveAs: (id, name) => this.changed(invoke<FileMeta | null>('files_save_dialog', { id, name })),
    saveCopy: (id, name) => invoke<boolean>('files_copy_dialog', { id, name }),
    access: async () => 'granted',
    requestAccess: async () => true,
  };

  /** Изменение списка — своему экрану. */
  private async changed<T>(p: Promise<T>): Promise<T> {
    const v = await p;
    for (const l of this.listeners) l();
    return v;
  }

  list(section: Section, query: string, start: number, count: number) {
    return invoke<IoResults['list']>('files_list', { section, query, start, count });
  }
  get(id: string) {
    return invoke<FileMeta | null>('files_get', { id });
  }
  async create(name: string, bytes?: Blob) {
    const meta = await invoke<FileMeta>('files_create', { name });
    // «Импорт…»: содержимое файла с диска — сразу записью (чтение Blob — ввод-вывод, не разбор).
    if (bytes) return this.changed(raw<FileMeta>('files_write', await bytes.arrayBuffer(), { id: meta.id, base: meta.version }));
    for (const l of this.listeners) l();
    return meta;
  }
  async read(id: string) {
    const bytes = bytesOrNull(await invoke<ArrayBuffer>('files_read', { id }));
    const meta = await this.get(id);
    if (!meta) throw new Error('Файл не найден');
    // Наибольший сеанс среди пачек — и без байтов файла (новый сеанс берётся после него).
    const session = await invoke<number | null>('files_journal_session', { id });
    const journal: ArrayBuffer[] = [];
    if (bytes) {
      for (const name of await invoke<string[]>('files_journal_names', { id })) {
        journal.push(await invoke<ArrayBuffer>('files_journal_read', { id, name }));
      }
    }
    return { meta, bytes, journal, session };
  }
  async write(
    id: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    const args = { id, base, ...(journal ? { session: journal.session, upTo: journal.upTo } : {}) };
    let meta = await raw<FileMeta>('files_write', bytes, args);
    if (thumb !== 'keep') {
      // Файл уже записан (поколение выросло): отказ превью — не отказ записи, иначе
      // следующая запись со старым поколением получила бы ложный CONFLICT.
      try {
        meta = await raw<FileMeta>('files_write_thumb', thumb ?? new ArrayBuffer(0), { id });
      } catch (e) {
        console.warn('Превью не записано:', e instanceof Error ? e.message : e);
      }
    }
    for (const l of this.listeners) l();
    return meta;
  }
  async appendJournal(id: string, mark: JournalMark, bytes: ArrayBuffer) {
    await raw<null>('files_journal_append', bytes, { id, session: mark.session, batch: mark.batch });
  }
  async blob(id: string) {
    const b = bytesOrNull(await invoke<ArrayBuffer>('files_read', { id }));
    return b ? new Blob([b], { type: 'application/octet-stream' }) : null;
  }
  rename(id: string, name: string) {
    return this.changed(invoke<FileMeta>('files_rename', { id, name }));
  }
  touch(id: string) {
    return this.changed(invoke<FileMeta>('files_touch', { id }));
  }
  async trash(id: string) {
    await this.changed(invoke<null>('files_trash', { id, on: true }));
  }
  async restore(id: string) {
    await this.changed(invoke<null>('files_trash', { id, on: false }));
  }
  async remove(id: string) {
    await this.changed(invoke<null>('files_delete', { id }));
  }
  async emptyTrash() {
    await this.changed(invoke<null>('files_empty_trash'));
  }
  async thumbnail(id: string) {
    const b = bytesOrNull(await invoke<ArrayBuffer>('files_thumb', { id }));
    return b ? new Blob([b], { type: 'image/png' }) : null;
  }
  onChange(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  dispose() {
    this.listeners.clear();
  }

  // Настройки — файл конфигурации; окно одно — чужих изменений нет.
  settings() {
    return invoke<Record<string, unknown>>('settings_get');
  }
  async setSettings(patch: Partial<Settings>) {
    await invoke<null>('settings_set', { patch });
  }
  onSettings() {
    return () => {};
  }
}

const same = (b: ArrayBuffer | null | undefined, want: number[]) =>
  !!b && b.byteLength === want.length && new Uint8Array(b).every((v, i) => v === want[i]);

/** Круг через IPC во временной библиотеке проверки: запись сырым телом с заголовком,
 * превью, пачка журнала, чтение байтов (`ArrayBuffer`), сеанса и журнала. */
async function ioRoundTrip(s: TauriFileStore): Promise<true | string> {
  const m = await s.create('Проверка запуска');
  const w = await s.write(m.id, new Uint8Array([1, 2, 3]).buffer, new Uint8Array([9]).buffer, m.version, null);
  if (w.version !== m.version + 1) return `поколение ${w.version}`;
  await s.appendJournal(m.id, { session: 7, batch: 1 }, new Uint8Array([4, 5]).buffer);
  const r = await s.read(m.id);
  if (!same(r.bytes, [1, 2, 3])) return 'байты файла';
  if (r.session !== 7 || r.journal.length !== 1 || !same(r.journal[0], [4, 5])) return 'журнал';
  if ((await s.thumbnail(m.id))?.size !== 1) return 'превью';
  await s.remove(m.id);
  return true;
}

/** Проверка запуска десктопа (`FERRUM_SMOKE=1`, `apps/desktop`): новый файл, ядро
 * готово, круг ввода-вывода через IPC — отчёт приложению, оно печатает его и выходит. */
export async function runSmoke(): Promise<void> {
  if (!isTauri() || !(await invoke<boolean>('smoke_mode'))) return;
  location.hash = '#/new';
  const t0 = performance.now();
  const ready = () => (window as { __ferrum?: { ready: boolean } }).__ferrum?.ready === true;
  while (performance.now() - t0 < 60_000 && !ready()) await new Promise((r) => setTimeout(r, 100));
  const s = new TauriFileStore();
  let files = -1;
  try {
    files = (await s.list('drafts', '', 0, 10)).total;
  } catch {
    files = -1;
  }
  let io: true | string;
  try {
    io = await ioRoundTrip(s);
  } catch (e) {
    io = e instanceof Error ? e.message : String(e);
  }
  await invoke<null>('smoke_done', {
    report: { ready: ready(), isolated: crossOriginIsolated, files, io, ms: Math.round(performance.now() - t0) },
  });
}
