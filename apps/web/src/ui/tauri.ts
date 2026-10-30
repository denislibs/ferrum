// Десктоп (Tauri 2, этап 6): хранилище файлов и настройки — в Rust (`apps/desktop`,
// крейт `ferrum-desktop-store`), интерфейс обращается к ним командами IPC. IPC есть
// только у окна, поэтому байты файла, журнала и превью идут в Rust из основного потока
// тем же `ArrayBuffer` сырым телом запроса (без JSON и без копий в JS), а запись,
// `fsync` и переименование — в потоке Tauri. Пути на диске интерфейс не видит и не
// передаёт: их дают только родные диалоги в Rust. Окно одно: изменения списка
// интерфейс узнаёт от своих же вызовов, `BroadcastChannel` не нужен.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
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
    throw new Error(russian(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)));
  }
}

/** Отказы самого Tauri (до кода команды) — английские: неверные аргументы, нет такой
 * команды, хранилище не открыто. Свои отказы Rust — уже по-русски, без изменений. */
function russian(message: string): string {
  if (message.startsWith('state not managed')) return 'Хранилище файлов не открыто';
  if (/^invalid args |^command \S+ not found/.test(message)) return 'Неверный запрос к хранилищу файлов';
  return message;
}

/** Системные шрифты десктопа (спека подсистемы 3, раздел 4): семейства из
 * каталогов шрифтов; файл начертания — номер файла в списке Rust. */
export function systemFontList(): Promise<FontFamilyDto[]> {
  return invoke<FontFamilyDto[]>('fonts_list');
}

/** Байты файла системного шрифта — сырым ответом IPC. */
export function systemFontBytes(file: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('fonts_read', { id: Number(file) });
}

/** Сырое тело запроса и параметры заголовком `x-ferrum` (ASCII: UUID и числа). */
function raw<T>(cmd: string, bytes: ArrayBuffer, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, new Uint8Array(bytes), { 'x-ferrum': JSON.stringify(args) });
}

const bytesOrNull = (b: ArrayBuffer) => (b.byteLength > 0 ? b : null);

/** Чтение, запись и журнал — по одному, в порядке вызова (как `serial` worker'а
 * ввода-вывода): чтение после сбоя ядра не встаёт между записью файла и записью
 * превью прежнего редактора, пачка, отправленная раньше чтения, в нём уже есть.
 * Очередь одна на окно (не на экземпляр): Rust-хранилище тоже одно. */
let queue: Promise<unknown> = Promise.resolve();
function serial<T>(fn: () => Promise<T>): Promise<T> {
  const p = queue.then(fn, fn);
  queue = p.catch(() => {});
  return p;
}

/** Ответ `files_read_open`: снимок под одной блокировкой библиотеки; байты — `parts`
 * частей чтения `ticket` (сначала файл, если `bytes`, затем пачки журнала). */
interface ReadHead {
  meta: FileMeta;
  session: number | null;
  ticket: number;
  parts: number;
  bytes: boolean;
  journalDropped: boolean;
}

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
  read(id: string) {
    return serial(async () => {
      // Метаданные, сеанс (наибольший среди пачек — и без байтов файла) и журнал —
      // одним снимком в Rust; части забираются сырыми ответами, без JSON и копий.
      const head = await invoke<ReadHead>('files_read_open', { id });
      const parts: ArrayBuffer[] = [];
      for (let i = 0; i < head.parts; i++) parts.push(await invoke<ArrayBuffer>('files_read_part', { ticket: head.ticket }));
      const file = head.bytes ? parts.shift() : undefined;
      const bytes = file ? bytesOrNull(file) : null;
      return { meta: head.meta, bytes, journal: bytes ? parts : [], session: head.session, journalTooLarge: head.journalDropped };
    });
  }
  write(
    id: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    return serial(() => this.writeNow(id, bytes, thumb, base, journal));
  }
  private async writeNow(
    id: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    const args = { id, base, ...(journal ? { session: journal.session, upTo: journal.upTo } : {}) };
    let meta: FileMeta & { warning?: string } = await raw<FileMeta>('files_write', bytes, args);
    if (thumb !== 'keep') {
      // Файл уже записан (поколение выросло): отказ превью — не отказ записи, иначе
      // следующая запись со старым поколением получила бы ложный CONFLICT. Причину
      // показывает сохранитель — предупреждением.
      try {
        meta = await raw<FileMeta>('files_write_thumb', thumb ?? new ArrayBuffer(0), { id });
      } catch (e) {
        meta = { ...meta, warning: `Превью файла не записано: ${e instanceof Error ? e.message : String(e)}` };
      }
    }
    for (const l of this.listeners) l();
    return meta;
  }
  async appendJournal(id: string, mark: JournalMark, bytes: ArrayBuffer) {
    await serial(() => raw<null>('files_journal_append', bytes, { id, session: mark.session, batch: mark.batch }));
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

/** Долгая запись для замера главного потока: 128 МБ (запись и `fsync` — сотни мс). */
const LONG_WRITE_BYTES = 128 * 1024 * 1024;

/** Круг через IPC во временной библиотеке проверки: запись сырым телом с заголовком,
 * превью, пачка журнала, чтение байтов (`ArrayBuffer`), сеанса и журнала. Затем долгая
 * запись, а во время неё — `files_get` (ждёт библиотеку) и `smoke_mode` (синхронная:
 * выполняется в главном потоке). Если бы команда, ждущая библиотеку, шла в главном
 * потоке, `smoke_mode` ждала бы конца записи: `pingMs` — худшая задержка ответа. */
async function ioRoundTrip(s: TauriFileStore): Promise<{ io: true | string; writeMs: number; pingMs: number }> {
  const m = await s.create('Проверка запуска');
  const w = await s.write(m.id, new Uint8Array([1, 2, 3]).buffer, new Uint8Array([9]).buffer, m.version, null);
  const fail = (io: string) => ({ io, writeMs: -1, pingMs: -1 });
  if (w.version !== m.version + 1) return fail(`поколение ${w.version}`);
  await s.appendJournal(m.id, { session: 7, batch: 1 }, new Uint8Array([4, 5]).buffer);
  const r = await s.read(m.id);
  if (!same(r.bytes, [1, 2, 3])) return fail('байты файла');
  if (r.session !== 7 || r.journal.length !== 1 || !same(r.journal[0], [4, 5])) return fail('журнал');
  if ((await s.thumbnail(m.id))?.size !== 1) return fail('превью');
  const t0 = performance.now();
  let writing = true;
  const long = s.write(m.id, new ArrayBuffer(LONG_WRITE_BYTES), 'keep', w.version, null).finally(() => {
    writing = false;
  });
  let pingMs = 0;
  while (writing) {
    const get = s.get(m.id);
    const p0 = performance.now();
    await invoke<boolean>('smoke_mode');
    if (writing) pingMs = Math.max(pingMs, performance.now() - p0);
    await get;
  }
  if ((await long).version !== w.version + 1) return fail('долгая запись');
  const writeMs = Math.round(performance.now() - t0);
  await s.remove(m.id);
  return { io: true, writeMs, pingMs: Math.round(pingMs) };
}

/** CSP действует: без `'unsafe-eval'` код из строки запрещён. */
function cspActive(): boolean {
  try {
    new Function('return 1')();
    return false;
  } catch {
    return true;
  }
}

/** PNG 1 × 1 — превью карточки на экране файлов проверки. */
const PNG_1PX = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

/** Текст из подмножеств шрифтов, встроенных в CSS как `data:`: кириллица-доп.
 * (ѣ U+0463, ӕ U+04D5 — `data:` и woff2, и woff) и вьетнамский (ệ U+1EC7 — `data:` woff2). */
const RARE_GLYPHS = 'ѣ ӕ ệ';

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Экран файлов под CSP: карточка с превью (`blob:`) и именем с редкими символами,
 * шрифты этих символов (моноширинный встроен `data:`). `true` — превью показано и
 * шрифты загружены; иначе — что не так. */
async function filesScreen(s: TauriFileStore): Promise<true | string> {
  const m = await s.create(`Проверка ${RARE_GLYPHS}`);
  const png = Uint8Array.from(atob(PNG_1PX), (c) => c.charCodeAt(0));
  await s.write(m.id, new Uint8Array([1, 2, 3]).buffer, png.buffer, m.version, null);
  location.hash = '#/';
  const shown = () =>
    [...document.querySelectorAll('img')].some((i) => i.src.startsWith('blob:') && i.complete && i.naturalWidth > 0);
  const t0 = performance.now();
  while (performance.now() - t0 < 15_000 && !shown()) await sleep(100);
  if (!shown()) return 'превью на экране файлов не показано';
  if (!document.body.textContent?.includes(RARE_GLYPHS)) return 'имя файла не показано';
  // Моноширинный шрифт для тех же символов: подмножества — `data:` в CSS.
  const probe = document.createElement('span');
  probe.style.fontFamily = 'var(--font-mono)';
  probe.textContent = RARE_GLYPHS;
  document.body.append(probe);
  try {
    for (const weight of [400, 500]) {
      const faces = await document.fonts.load(`${weight} 16px "JetBrains Mono"`, RARE_GLYPHS);
      if (faces.length === 0 || faces.some((f) => f.status !== 'loaded')) return `шрифт ${weight} не загружен`;
    }
  } catch (e) {
    return `шрифт: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    probe.remove();
  }
  return true;
}

/** Проверка запуска десктопа (`FERRUM_SMOKE=1`, `apps/desktop`): новый файл, ядро
 * готово, круг ввода-вывода через IPC, экран файлов с превью и редкими символами,
 * нарушения CSP — отчёт приложению, оно печатает его и выходит. */
export async function runSmoke(): Promise<void> {
  if (!isTauri()) return;
  // Нарушения CSP страницы — с первого кадра (до ответа `smoke_mode`).
  const csp: string[] = [];
  document.addEventListener('securitypolicyviolation', (e) => csp.push(`${e.effectiveDirective} ${e.blockedURI}`));
  if (!(await invoke<boolean>('smoke_mode'))) return;
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
  let round: { io: true | string; writeMs: number; pingMs: number };
  try {
    round = await ioRoundTrip(s);
  } catch (e) {
    round = { io: e instanceof Error ? e.message : String(e), writeMs: -1, pingMs: -1 };
  }
  const isReady = ready();
  let screen: true | string;
  try {
    screen = await filesScreen(s);
  } catch (e) {
    screen = e instanceof Error ? e.message : String(e);
  }
  // События нарушений CSP приходят асинхронно — дать им дойти.
  await sleep(300);
  await invoke<null>('smoke_done', {
    report: { ready: isReady, isolated: crossOriginIsolated, files, ...round, screen, csp, cspActive: cspActive(), ms: Math.round(performance.now() - t0) },
  });
}
