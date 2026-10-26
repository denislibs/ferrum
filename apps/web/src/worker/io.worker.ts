// Worker ввода-вывода (спека 4.2, 10): хранилище файлов в IndexedDB. Основной поток
// только шлёт запросы и получает метаданные, окна списка и превью; байты файлов
// приходят и уходят передачей (transfer), без копий в основном потоке.
//
// База `ferrum`, версия 4:
// - `meta` — записи `FileMeta` (ключ `id`): список читает только их;
// - `bytes` — id → ArrayBuffer файла `.ferrum`;
// - `thumbs` — id → Blob PNG превью;
// - `journal` (с версии 2) — `[id, сеанс, номер]` → ArrayBuffer пачки журнала
//   изменений: правки после байтов файла, по порядку записи. Запись файла стирает
//   пачки до номера своего снимка в той же транзакции;
// - `settings` (с версии 3) — поле настроек пользователя → значение: вкладки пишут
//   только изменённые поля, другие узнают через `BroadcastChannel('ferrum-settings')`;
// - `handles` (с версии 4) — id → `{ handle, lastModified }` записи, связанной с
//   файлом на диске (File System Access): байты — в файле, не в `bytes`; запись — через
//   `createWritable` (браузер подменяет файл целиком при `close`), чужая правка файла
//   видна по `lastModified`.
// Метаданные держатся в памяти (≤ 10 000 записей — сотни килобайт): сортировка,
// поиск и окна — отсюда. Другие вкладки узнают об изменениях через
// `BroadcastChannel('ferrum-files')` и перечитывают метаданные.
//
// Правки записей идут по одной (очередь) и читают запись из базы в той же
// транзакции, где пишут: одновременные запись и переименование не откатывают поля
// друг друга, а устаревшая память (другая вкладка уже записала) не пишется поверх.
// Запись содержимого несёт поколение, на котором основана (`base`); не совпало —
// отказ `CONFLICT`. Транзакции записи — `durability: 'strict'`: это единственная
// копия файла.

import {
  CONFLICT,
  DISK_ACCESS,
  DISK_CHANGED,
  DISK_MISSING,
  DISK_TAKEN,
  GONE,
  nameFromFile,
  cleanName,
  requestError,
  sectionList,
  windowOf,
  UNTITLED,
  type FileMeta,
  type FromIo,
  type IoRequest,
  type IoResults,
  type ToIo,
} from '../core/file-list';
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES, tooLarge } from '../core/limits';

const scope = self as unknown as DedicatedWorkerGlobalScope;
const DB_NAME = 'ferrum';
const DB_VERSION = 4;
const CHANNEL = 'ferrum-files';
const SETTINGS_CHANNEL = 'ferrum-settings';

let test = false;
let failWrites = false;
let holdSettings = false;
let db: Promise<IDBDatabase> | null = null;
const metas = new Map<string, FileMeta>();
let loaded: Promise<void> | null = null;
const channel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(CHANNEL) : null;
const settingsChannel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(SETTINGS_CHANNEL) : null;

function post(msg: FromIo, transfer: Transferable[] = []): void {
  scope.postMessage(msg, transfer);
}

/** Ошибка IndexedDB — словами для пользователя (квота, закрытое хранилище). */
function storageError(e: unknown): Error {
  const name = e instanceof DOMException ? e.name : '';
  if (name === 'QuotaExceededError') return new Error('недостаточно места в хранилище браузера');
  if (name === 'InvalidStateError' || name === 'SecurityError') return new Error('хранилище браузера недоступно');
  // Другая вкладка с более новой версией приложения уже обновила схему базы.
  if (name === 'VersionError') return new Error('хранилище обновлено в другой вкладке — обновите страницу');
  return new Error(e instanceof Error && e.message ? e.message : 'ошибка хранилища браузера');
}

function open(): Promise<IDBDatabase> {
  db ??= new Promise<IDBDatabase>((resolve, reject) => {
    let req: IDBOpenDBRequest;
    let gaveUp = false;
    try {
      req = indexedDB.open(DB_NAME, DB_VERSION);
    } catch (e) {
      reject(storageError(e));
      return;
    }
    req.onupgradeneeded = () => {
      const d = req.result;
      if (!d.objectStoreNames.contains('meta')) d.createObjectStore('meta', { keyPath: 'id' });
      if (!d.objectStoreNames.contains('bytes')) d.createObjectStore('bytes');
      if (!d.objectStoreNames.contains('thumbs')) d.createObjectStore('thumbs');
      if (!d.objectStoreNames.contains('journal')) d.createObjectStore('journal');
      if (!d.objectStoreNames.contains('settings')) d.createObjectStore('settings');
      if (!d.objectStoreNames.contains('handles')) d.createObjectStore('handles');
    };
    req.onsuccess = () => {
      const d = req.result;
      // Открытие уже отклонено (`onblocked`) — позднее соединение не нужно: закрыть,
      // иначе оно держало бы базу и мешало следующему обновлению схемы.
      if (gaveUp) {
        d.close();
        return;
      }
      // Другая вкладка обновляет схему: уступаем, при следующем запросе откроемся заново.
      d.onversionchange = () => {
        d.close();
        db = null;
      };
      resolve(d);
    };
    req.onerror = () => reject(storageError(req.error));
    // Старая вкладка держит прежнюю версию базы и не уступает (вкладки этого приложения
    // закрывают соединение по `versionchange`): отказ сразу, а не вечное ожидание.
    req.onblocked = () => {
      gaveUp = true;
      reject(new Error('хранилище занято другой вкладкой — закройте её или обновите'));
    };
  });
  // Неудачное открытие не кэшируем: следующая попытка откроет заново.
  db.catch(() => (db = null));
  return db;
}

function done(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(storageError(tx.error));
    tx.onabort = () => reject(storageError(tx.error));
  });
}

function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(storageError(r.error));
  });
}

/** Запись из базы — не доверенная (чужая версия, ручная правка): только верные поля. */
function validMeta(v: unknown): FileMeta | null {
  if (typeof v !== 'object' || v === null) return null;
  const m = v as Record<string, unknown>;
  const num = (x: unknown) => typeof x === 'number' && Number.isFinite(x);
  if (typeof m.id !== 'string' || typeof m.name !== 'string') return null;
  if (!num(m.createdAt) || !num(m.editedAt) || !num(m.openedAt) || !num(m.size) || !num(m.thumbVersion)) return null;
  const meta: FileMeta = {
    id: m.id,
    name: cleanName(m.name),
    createdAt: m.createdAt as number,
    editedAt: m.editedAt as number,
    openedAt: m.openedAt as number,
    size: m.size as number,
    thumbVersion: m.thumbVersion as number,
    version: num(m.version) ? (m.version as number) : 0,
  };
  if (num(m.trashedAt)) meta.trashedAt = m.trashedAt as number;
  if (typeof m.disk === 'string' && m.disk.length > 0 && m.disk.length <= 4096) meta.disk = m.disk;
  return meta;
}

async function reload(): Promise<void> {
  const d = await open();
  const all = await request(d.transaction('meta').objectStore('meta').getAll());
  metas.clear();
  for (const v of all) {
    const m = validMeta(v);
    if (m) metas.set(m.id, m);
  }
}

function ready(): Promise<void> {
  loaded ??= reload().catch((e: unknown) => {
    loaded = null;
    throw e;
  });
  return loaded;
}

/** Изменение списка: своему экрану — сразу, другим вкладкам — через канал. */
function changed(): void {
  post({ t: 'changed' });
  channel?.postMessage({ t: 'changed' });
}

function meta(id: string): FileMeta {
  const m = metas.get(id);
  if (!m) throw new Error('Файл не найден');
  return m;
}

function rw(d: IDBDatabase, stores: string[]): IDBTransaction {
  return d.transaction(stores, 'readwrite', { durability: 'strict' });
}

/** Правка одной записи: свежая запись читается в той же транзакции, `edit` даёт
 * новую (и может дописать в другие хранилища той же транзакции) или бросает. */
async function update(
  id: string,
  stores: string[],
  edit: (m: FileMeta, tx: IDBTransaction) => FileMeta,
  missing = 'Файл не найден',
): Promise<FileMeta> {
  const d = await open();
  return new Promise<FileMeta>((resolve, reject) => {
    const tx = rw(d, ['meta', ...stores]);
    let next: FileMeta | null = null;
    let failure: Error | null = null;
    const req = tx.objectStore('meta').get(id);
    req.onsuccess = () => {
      const cur = validMeta(req.result);
      try {
        if (!cur) throw new Error(missing);
        next = edit(cur, tx);
        tx.objectStore('meta').put(next);
      } catch (e) {
        failure = e instanceof Error ? e : new Error(String(e));
        tx.abort();
      }
    };
    tx.oncomplete = () => {
      metas.set(id, next!);
      resolve(next!);
    };
    tx.onabort = () => reject(failure ?? storageError(tx.error));
  });
}

/** Правки — по одной, в порядке прихода. */
let queue: Promise<unknown> = Promise.resolve();
function serial<T>(fn: () => Promise<T>): Promise<T> {
  const p = queue.then(fn, fn);
  queue = p.catch(() => {});
  return p;
}

interface DiskRecord {
  handle: FileSystemFileHandle;
  /** `lastModified` файла после нашего последнего чтения или записи; `null` — не
   * известно (файл записан, а прочитать его после записи не удалось): проверка чужой
   * правки пропускается до следующего чтения или записи, которые его узнают. */
  lastModified: number | null;
}

/** Ошибка File System Access — словами для пользователя. */
function diskError(e: unknown): Error {
  const name = e instanceof DOMException ? e.name : '';
  if (name === 'NotFoundError') return new Error(DISK_MISSING);
  if (name === 'NotAllowedError' || name === 'SecurityError') return new Error(DISK_ACCESS);
  return storageError(e);
}

async function diskOf(id: string): Promise<DiskRecord | null> {
  const d = await open();
  const v = (await request(d.transaction('handles').objectStore('handles').get(id))) as unknown;
  const r = v as Partial<DiskRecord> | undefined;
  return r && r.handle instanceof FileSystemFileHandle && (typeof r.lastModified === 'number' || r.lastModified === null)
    ? { handle: r.handle, lastModified: r.lastModified }
    : null;
}

/** Записи нет (удалена в другой вкладке): убрать и её строку дескриптора — в той же
 * транзакции, где проверено, что записи нет. */
async function dropOrphan(id: string): Promise<void> {
  const d = await open();
  const tx = rw(d, ['meta', 'handles']);
  const req = tx.objectStore('meta').get(id);
  req.onsuccess = () => {
    if (!validMeta(req.result)) tx.objectStore('handles').delete(id);
  };
  await done(tx);
}

async function fileOf(handle: FileSystemFileHandle): Promise<File> {
  try {
    return await handle.getFile();
  } catch (e) {
    throw diskError(e);
  }
}

/** Записать байты в файл на диске: браузер пишет во временный и подменяет им файл при
 * `close`. Ответ — `lastModified` записанного файла; `null` — запись удалась (`close`
 * прошёл), а прочитать файл после неё не вышло: это не отказ записи. */
async function writeDisk(handle: FileSystemFileHandle, bytes: ArrayBuffer): Promise<number | null> {
  let w: FileSystemWritableFileStream;
  try {
    w = await handle.createWritable();
  } catch (e) {
    throw diskError(e);
  }
  try {
    await w.write(bytes);
    await w.close();
  } catch (e) {
    await w.abort().catch(() => {});
    throw diskError(e);
  }
  try {
    return (await handle.getFile()).lastModified;
  } catch {
    return null;
  }
}

/** Все пачки журнала файла: ключи `[id, сеанс, номер]`, массив больше любого числа. */
function journalRange(id: string): IDBKeyRange {
  return IDBKeyRange.bound([id], [id, []]);
}

/** Байты файла и пачки журнала после них — одной транзакцией. Журнал без байтов не
 * отдаётся: его пачки начаты на снимке, которого нет. Пачек больше
 * `MAX_JOURNAL_ENTRIES` — не читаются вовсе (сначала дешёвый `count`): файл
 * открывается без журнала, с уведомлением (`journalTooLarge`). */
async function readFile(id: string): Promise<{ value: IoResults['read']; transfer: Transferable[] }> {
  const m = meta(id);
  const d = await open();
  const tx = d.transaction(['bytes', 'journal']);
  const journalStore = tx.objectStore('journal');
  const [raw, entries, last] = await Promise.all([
    request(tx.objectStore('bytes').get(id)) as Promise<unknown>,
    // `getAll` — в обработчике ответа `count`: транзакция ещё активна.
    request(journalStore.count(journalRange(id))).then((n) =>
      n > MAX_JOURNAL_ENTRIES ? null : (request(journalStore.getAll(journalRange(id))) as Promise<unknown[]>),
    ),
    // Последний ключ `[id, сеанс, номер]` — наибольший сеанс (ключи упорядочены).
    request(tx.objectStore('journal').openKeyCursor(journalRange(id), 'prev')).then((c) => c?.key),
  ]);
  let bytes = raw instanceof ArrayBuffer ? raw : null;
  const disk = m.disk ? await diskOf(id) : null;
  if (disk) {
    // Байты — в файле на диске; пустой файл (только что выбранный для сохранения) — новый.
    const f = await fileOf(disk.handle);
    if (f.size > MAX_OPEN_BYTES) throw new Error(tooLarge(MAX_OPEN_BYTES, f.size));
    bytes = f.size > 0 ? await f.arrayBuffer() : null;
    if (f.lastModified !== disk.lastModified) {
      const w = rw(d, ['handles']);
      w.objectStore('handles').put({ handle: disk.handle, lastModified: f.lastModified }, id);
      await done(w);
    }
  }
  const journal = bytes && entries ? entries.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer) : [];
  const journalTooLarge = !!bytes && entries === null;
  const s = Array.isArray(last) ? last[1] : null;
  const session = typeof s === 'number' && Number.isSafeInteger(s) && s >= 0 ? s : null;
  return { value: { meta: m, bytes, journal, session, journalTooLarge }, transfer: [...(bytes ? [bytes] : []), ...journal] };
}

async function readBytes(id: string): Promise<ArrayBuffer | null> {
  const d = await open();
  const bytes = (await request(d.transaction('bytes').objectStore('bytes').get(id))) as unknown;
  return bytes instanceof ArrayBuffer ? bytes : null;
}

async function handle(r: IoRequest): Promise<{ value: unknown; transfer?: Transferable[] }> {
  await ready();
  switch (r.op) {
    case 'list': {
      const list = sectionList(metas.values(), r.section, r.query);
      return { value: { total: list.length, rows: windowOf(list, r.start, r.count) } };
    }
    case 'get':
      return { value: metas.get(r.file) ?? null };
    case 'read':
      // В очереди правок: пачка журнала, отправленная раньше чтения, в нём уже есть
      // (после сбоя ядра редактор читает файл сразу).
      return serial(() => readFile(r.file));
    case 'blob': {
      // Файл на диске — уже Blob (`File`): основной поток его не копирует.
      if (meta(r.file).disk) {
        const disk = await diskOf(r.file);
        if (disk) return { value: await fileOf(disk.handle) };
      }
      const buf = await readBytes(r.file);
      // Blob собирается здесь: основной поток не копирует сотни мегабайт.
      return { value: buf ? new Blob([buf], { type: 'application/octet-stream' }) : null };
    }
    case 'handle':
      meta(r.file);
      return { value: (await diskOf(r.file))?.handle ?? null };
    case 'settings': {
      const d = await open();
      const st = d.transaction('settings').objectStore('settings');
      const [keys, values] = await Promise.all([request(st.getAllKeys()), request(st.getAll())]);
      const out: Record<string, unknown> = {};
      keys.forEach((k, i) => {
        if (typeof k === 'string') out[k] = values[i];
      });
      return { value: out };
    }
    case 'thumb': {
      const m = metas.get(r.file);
      if (!m || m.thumbVersion === 0) return { value: null };
      const d = await open();
      const blob = (await request(d.transaction('thumbs').objectStore('thumbs').get(r.file))) as unknown;
      return { value: blob instanceof Blob ? blob : null };
    }
    case 'setSettings':
      // Тесты: запись «потеряна» — ответа нет (вне очереди правок: её не держит).
      if (holdSettings) return new Promise<never>(() => {});
      return serial(() => mutate(r));
    default:
      return serial(() => mutate(r));
  }
}

async function mutate(r: IoRequest): Promise<{ value: unknown }> {
  const now = Date.now();
  switch (r.op) {
    case 'create': {
      const bytes = r.bytes instanceof Blob ? await r.bytes.arrayBuffer() : r.bytes;
      const m: FileMeta = {
        id: crypto.randomUUID(),
        name: cleanName(r.name),
        createdAt: now,
        editedAt: now,
        openedAt: now,
        size: bytes?.byteLength ?? 0,
        thumbVersion: 0,
        version: bytes ? 1 : 0,
      };
      const d = await open();
      const tx = rw(d, ['meta', 'bytes']);
      tx.objectStore('meta').put(m);
      if (bytes) tx.objectStore('bytes').put(bytes, m.id);
      await done(tx);
      metas.set(m.id, m);
      changed();
      return { value: m };
    }
    case 'write': {
      if (failWrites) throw new Error('недостаточно места в хранилище браузера');
      const { bytes, thumb, base, journal } = r;
      // Файл на диске: сначала он (атомарно), затем метаданные и журнал в базе. Сбой
      // между ними оставит пачки, уже вошедшие в файл, — их повтор безвреден.
      let disk: DiskRecord | null = null;
      if (metas.get(r.file)?.disk) {
        const rec = await diskOf(r.file);
        if (rec) {
          // Поколение — из базы, а не из памяти: другая вкладка могла записать файл, а
          // этот worker ещё не перечитал список. Иначе её запись на диске была бы затёрта
          // раньше, чем отказ `CONFLICT` из транзакции ниже.
          const d = await open();
          const cur = validMeta(await request(d.transaction('meta').objectStore('meta').get(r.file)));
          if (!cur) {
            await dropOrphan(r.file).catch(() => {});
            throw new Error(GONE);
          }
          if (cur.version !== base) throw new Error(CONFLICT);
          if (rec.lastModified !== null && (await fileOf(rec.handle)).lastModified !== rec.lastModified) throw new Error(DISK_CHANGED);
          disk = { handle: rec.handle, lastModified: await writeDisk(rec.handle, bytes) };
        }
      }
      // Имя записи здесь не меняется: оно — из хранилища (переименование — `rename`,
      // в том числе из другой вкладки), снимок его не затирает.
      const next = await update(
        r.file,
        ['bytes', 'thumbs', 'journal', 'handles'],
        (m, tx) => {
          if (m.version !== base) throw new Error(CONFLICT);
          if (disk) tx.objectStore('handles').put(disk, m.id);
          else tx.objectStore('bytes').put(bytes, m.id);
          // Пачки, вошедшие в снимок, — в той же транзакции: файл и журнал не расходятся.
          if (journal) tx.objectStore('journal').delete(IDBKeyRange.bound([m.id], [m.id, journal.session, journal.upTo]));
          if (thumb instanceof ArrayBuffer) tx.objectStore('thumbs').put(new Blob([thumb], { type: 'image/png' }), m.id);
          else if (thumb === null) tx.objectStore('thumbs').delete(m.id);
          return {
            ...m,
            editedAt: now,
            size: bytes.byteLength,
            version: m.version + 1,
            // Прежнее превью оставлено — версия та же, адрес картинки в интерфейсе тоже.
            thumbVersion: thumb === 'keep' ? m.thumbVersion : m.thumbVersion + 1,
          };
        },
        GONE,
      ).catch(async (e: unknown) => {
        // Файл на диске уже записан, а база — нет: его новое `lastModified` всё равно
        // запомнить, иначе своя же запись выглядела бы чужой правкой (`DISK_CHANGED`)
        // при каждой следующей. Поколение прежнее — следующая запись пройдёт.
        if (e instanceof Error && e.message === GONE) {
          // Записи больше нет: строку дескриптора не возвращать, а убрать.
          await dropOrphan(r.file).catch(() => {});
        } else if (disk) {
          const written = disk;
          await open()
            .then((d) => {
              const tx = rw(d, ['handles']);
              tx.objectStore('handles').put(written, r.file);
              return done(tx);
            })
            .catch(() => {});
        }
        throw e;
      });
      changed();
      return { value: next };
    }
    case 'createFromDisk': {
      // Тот же файл уже в списке — его запись (без дубля).
      for (const m of metas.values()) {
        if (!m.disk) continue;
        const rec = await diskOf(m.id);
        if (!rec || !(await rec.handle.isSameEntry(r.handle))) continue;
        // Запись в корзине: файл снова открыли с диска — вернуть её из корзины.
        if (m.trashedAt === undefined) return { value: m };
        const back = await update(m.id, [], (cur) => {
          const next = { ...cur };
          delete next.trashedAt;
          return next;
        });
        changed();
        return { value: back };
      }
      const f = await fileOf(r.handle);
      if (f.size > MAX_OPEN_BYTES) throw new Error(tooLarge(MAX_OPEN_BYTES, f.size));
      const m: FileMeta = {
        id: crypto.randomUUID(),
        name: nameFromFile(f.name),
        createdAt: now,
        editedAt: f.lastModified || now,
        openedAt: now,
        size: f.size,
        thumbVersion: 0,
        version: f.size > 0 ? 1 : 0,
        disk: f.name,
      };
      const d = await open();
      const tx = rw(d, ['meta', 'handles']);
      tx.objectStore('meta').put(m);
      tx.objectStore('handles').put({ handle: r.handle, lastModified: f.lastModified }, m.id);
      await done(tx);
      metas.set(m.id, m);
      changed();
      return { value: m };
    }
    case 'attachDisk': {
      // Файл другой записи не занять: запись затёрла бы единственную копию той (как в
      // десктопе). Вопрос браузера «заменить?» об этом не говорит.
      for (const m of metas.values()) {
        if (m.id === r.file || !m.disk) continue;
        const rec = await diskOf(m.id);
        if (rec && (await rec.handle.isSameEntry(r.handle))) throw new Error(DISK_TAKEN);
      }
      // Байты записи — в выбранный файл (их может не быть: новый файл, ещё не сохранённый).
      // Запись уже на диске — её байты в прежнем файле, не в базе.
      const cur = meta(r.file).disk ? await diskOf(r.file) : null;
      let bytes: ArrayBuffer | null;
      if (cur) {
        const f = await fileOf(cur.handle);
        bytes = f.size > 0 ? await f.arrayBuffer() : null;
      } else bytes = await readBytes(r.file);
      const lastModified = bytes ? await writeDisk(r.handle, bytes) : (await fileOf(r.handle)).lastModified;
      // Поколение не меняется: содержимое то же, открытый редактор пишет дальше.
      const next = await update(r.file, ['bytes', 'handles'], (m, tx) => {
        tx.objectStore('handles').put({ handle: r.handle, lastModified }, m.id);
        tx.objectStore('bytes').delete(m.id);
        return { ...m, disk: r.handle.name };
      });
      changed();
      return { value: next };
    }
    case 'appendJournal': {
      // Запись есть — по базе, в той же транзакции (удалённый навсегда, в том числе в
      // другой вкладке, файл журнал не воскрешает: осиротевших пачек нет).
      meta(r.file);
      const d = await open();
      const tx = rw(d, ['meta', 'journal']);
      let missing = false;
      const req = tx.objectStore('meta').get(r.file);
      req.onsuccess = () => {
        if (validMeta(req.result)) tx.objectStore('journal').put(r.bytes, [r.file, r.session, r.batch]);
        else {
          missing = true;
          tx.abort();
        }
      };
      await done(tx).catch((e: unknown) => {
        throw missing ? new Error('Файл не найден') : e;
      });
      return { value: null };
    }
    case 'setSettings': {
      const d = await open();
      const tx = rw(d, ['settings']);
      for (const [k, v] of Object.entries(r.patch)) tx.objectStore('settings').put(v, k);
      await done(tx);
      settingsChannel?.postMessage({ patch: r.patch });
      return { value: null };
    }
    case 'journalCount': {
      if (!test) throw new Error('Только в тестовом режиме');
      const d = await open();
      return { value: await request(d.transaction('journal').objectStore('journal').count(journalRange(r.file))) };
    }
    case 'rename': {
      const name = cleanName(r.name);
      if (name === meta(r.file).name) return { value: meta(r.file) };
      const next = await update(r.file, [], (m) => ({ ...m, name, editedAt: now }));
      changed();
      return { value: next };
    }
    case 'touch': {
      const next = await update(r.file, [], (m) => ({ ...m, openedAt: now }));
      changed();
      return { value: next };
    }
    case 'trash':
    case 'restore': {
      await update(r.file, [], (m) => {
        const next = { ...m };
        if (r.op === 'trash') next.trashedAt = now;
        else delete next.trashedAt;
        return next;
      });
      changed();
      return { value: null };
    }
    case 'delete':
    case 'emptyTrash': {
      const ids = r.op === 'delete' ? [meta(r.file).id] : [...metas.values()].filter((m) => m.trashedAt !== undefined).map((m) => m.id);
      const d = await open();
      const tx = rw(d, ['meta', 'bytes', 'thumbs', 'journal', 'handles']);
      for (const id of ids) {
        // Файл на диске не трогаем: удаляется только запись о нём.
        for (const st of ['meta', 'bytes', 'thumbs', 'handles']) tx.objectStore(st).delete(id);
        tx.objectStore('journal').delete(journalRange(id));
      }
      await done(tx);
      for (const id of ids) metas.delete(id);
      changed();
      return { value: null };
    }
    case 'seed': {
      if (!test) throw new Error('Засев доступен только в тестовом режиме');
      const list: FileMeta[] = [];
      for (let i = 0; i < r.count; i++) {
        const t = now - i * 60_000;
        list.push({
          id: crypto.randomUUID(),
          name: `${UNTITLED} ${i + 1}`,
          createdAt: t,
          editedAt: t,
          openedAt: t,
          size: 0,
          thumbVersion: 0,
          version: 0,
        });
      }
      const d = await open();
      const tx = rw(d, ['meta']);
      for (const m of list) tx.objectStore('meta').put(m);
      await done(tx);
      for (const m of list) metas.set(m.id, m);
      changed();
      return { value: null };
    }
    case 'failWrites':
      if (!test) throw new Error('Только в тестовом режиме');
      failWrites = r.on;
      return { value: null };
    case 'holdSettings':
      if (!test) throw new Error('Только в тестовом режиме');
      holdSettings = r.on;
      return { value: null };
    default:
      throw new Error('Неверный запрос к хранилищу файлов');
  }
}

if (settingsChannel) {
  // Поля настроек от другой вкладки — своему интерфейсу (он проверит значения).
  settingsChannel.onmessage = (e: MessageEvent<unknown>) => {
    const patch = (e.data as { patch?: unknown } | null)?.patch;
    if (typeof patch === 'object' && patch !== null && !Array.isArray(patch)) {
      post({ t: 'settings', patch: patch as Record<string, unknown> });
    }
  };
}

if (channel) {
  channel.onmessage = () => {
    // Другая вкладка изменила список: перечитать метаданные, затем сказать экрану.
    loaded = reload();
    loaded.then(
      () => post({ t: 'changed' }),
      () => (loaded = null),
    );
  };
}

scope.onmessage = (e: MessageEvent<unknown>) => {
  const msg = e.data as ToIo;
  if (typeof msg !== 'object' || msg === null) return;
  if (msg.t === 'init') {
    test = msg.test === true;
    return;
  }
  if (msg.t !== 'req') return;
  const error = requestError(msg);
  if (error) {
    const id = typeof (msg as { id?: unknown }).id === 'number' ? (msg as { id: number }).id : -1;
    post({ t: 'res', id, ok: false, message: error });
    return;
  }
  handle(msg).then(
    ({ value, transfer }) => post({ t: 'res', id: msg.id, ok: true, value }, transfer),
    (err: unknown) => post({ t: 'res', id: msg.id, ok: false, message: err instanceof Error ? err.message : String(err) }),
  );
};
