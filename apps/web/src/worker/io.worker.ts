// Worker ввода-вывода (спека 4.2, 10): хранилище файлов в IndexedDB. Основной поток
// только шлёт запросы и получает метаданные, окна списка и превью; байты файлов
// приходят и уходят передачей (transfer), без копий в основном потоке.
//
// База `ferrum`, версия 2:
// - `meta` — записи `FileMeta` (ключ `id`): список читает только их;
// - `bytes` — id → ArrayBuffer файла `.ferrum`;
// - `thumbs` — id → Blob PNG превью;
// - `journal` (с версии 2) — `[id, сеанс, номер]` → ArrayBuffer пачки журнала
//   изменений: правки после байтов файла, по порядку записи. Запись файла стирает
//   пачки до номера своего снимка в той же транзакции.
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
  GONE,
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

const scope = self as unknown as DedicatedWorkerGlobalScope;
const DB_NAME = 'ferrum';
const DB_VERSION = 2;
const CHANNEL = 'ferrum-files';

let test = false;
let failWrites = false;
let db: Promise<IDBDatabase> | null = null;
const metas = new Map<string, FileMeta>();
let loaded: Promise<void> | null = null;
const channel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(CHANNEL) : null;

function post(msg: FromIo, transfer: Transferable[] = []): void {
  scope.postMessage(msg, transfer);
}

/** Ошибка IndexedDB — словами для пользователя (квота, закрытое хранилище). */
function storageError(e: unknown): Error {
  const name = e instanceof DOMException ? e.name : '';
  if (name === 'QuotaExceededError') return new Error('недостаточно места в хранилище браузера');
  if (name === 'InvalidStateError' || name === 'SecurityError') return new Error('хранилище браузера недоступно');
  return new Error(e instanceof Error && e.message ? e.message : 'ошибка хранилища браузера');
}

function open(): Promise<IDBDatabase> {
  db ??= new Promise<IDBDatabase>((resolve, reject) => {
    let req: IDBOpenDBRequest;
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
    };
    req.onsuccess = () => {
      const d = req.result;
      // Другая вкладка обновляет схему: уступаем, при следующем запросе откроемся заново.
      d.onversionchange = () => {
        d.close();
        db = null;
      };
      resolve(d);
    };
    req.onerror = () => reject(storageError(req.error));
    req.onblocked = () => reject(new Error('хранилище занято другой вкладкой'));
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

/** Все пачки журнала файла: ключи `[id, сеанс, номер]`, массив больше любого числа. */
function journalRange(id: string): IDBKeyRange {
  return IDBKeyRange.bound([id], [id, []]);
}

/** Байты файла и пачки журнала после них — одной транзакцией. Журнал без байтов не
 * отдаётся: его пачки начаты на снимке, которого нет. */
async function readFile(id: string): Promise<{ value: IoResults['read']; transfer: Transferable[] }> {
  const m = meta(id);
  const d = await open();
  const tx = d.transaction(['bytes', 'journal']);
  const [raw, entries, last] = await Promise.all([
    request(tx.objectStore('bytes').get(id)) as Promise<unknown>,
    request(tx.objectStore('journal').getAll(journalRange(id))) as Promise<unknown[]>,
    // Последний ключ `[id, сеанс, номер]` — наибольший сеанс (ключи упорядочены).
    request(tx.objectStore('journal').openKeyCursor(journalRange(id), 'prev')).then((c) => c?.key),
  ]);
  const bytes = raw instanceof ArrayBuffer ? raw : null;
  const journal = bytes ? entries.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer) : [];
  const s = Array.isArray(last) ? last[1] : null;
  const session = typeof s === 'number' && Number.isSafeInteger(s) && s >= 0 ? s : null;
  return { value: { meta: m, bytes, journal, session }, transfer: [...(bytes ? [bytes] : []), ...journal] };
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
      meta(r.file);
      const buf = await readBytes(r.file);
      // Blob собирается здесь: основной поток не копирует сотни мегабайт.
      return { value: buf ? new Blob([buf], { type: 'application/octet-stream' }) : null };
    }
    case 'thumb': {
      const m = metas.get(r.file);
      if (!m || m.thumbVersion === 0) return { value: null };
      const d = await open();
      const blob = (await request(d.transaction('thumbs').objectStore('thumbs').get(r.file))) as unknown;
      return { value: blob instanceof Blob ? blob : null };
    }
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
      // Имя записи здесь не меняется: оно — из хранилища (переименование — `rename`,
      // в том числе из другой вкладки), снимок его не затирает.
      const next = await update(
        r.file,
        ['bytes', 'thumbs', 'journal'],
        (m, tx) => {
          if (m.version !== base) throw new Error(CONFLICT);
          tx.objectStore('bytes').put(bytes, m.id);
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
      );
      changed();
      return { value: next };
    }
    case 'appendJournal': {
      // Запись есть (удалённый навсегда файл журнал не воскрешает).
      meta(r.file);
      const d = await open();
      const tx = rw(d, ['journal']);
      tx.objectStore('journal').put(r.bytes, [r.file, r.session, r.batch]);
      await done(tx);
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
      const tx = rw(d, ['meta', 'bytes', 'thumbs', 'journal']);
      for (const id of ids) {
        for (const st of ['meta', 'bytes', 'thumbs']) tx.objectStore(st).delete(id);
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
    default:
      throw new Error('Неверный запрос к хранилищу файлов');
  }
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
