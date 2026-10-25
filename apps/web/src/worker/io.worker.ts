// Worker ввода-вывода (спека 4.2, 10): хранилище файлов в IndexedDB. Основной поток
// только шлёт запросы и получает метаданные, окна списка и превью; байты файлов
// приходят и уходят передачей (transfer), без копий в основном потоке.
//
// База `ferrum`, версия 1:
// - `meta` — записи `FileMeta` (ключ `id`): список читает только их;
// - `bytes` — id → ArrayBuffer файла `.ferrum`;
// - `thumbs` — id → Blob PNG превью.
// Метаданные держатся в памяти (≤ 10 000 записей — сотни килобайт): сортировка,
// поиск и окна — отсюда. Другие вкладки узнают об изменениях через
// `BroadcastChannel('ferrum-files')` и перечитывают метаданные.

import {
  cleanName,
  requestError,
  sectionList,
  windowOf,
  UNTITLED,
  type FileMeta,
  type FromIo,
  type IoRequest,
  type ToIo,
} from '../core/file-list';

const scope = self as unknown as DedicatedWorkerGlobalScope;
const DB_NAME = 'ferrum';
const DB_VERSION = 1;
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

async function putMeta(...list: FileMeta[]): Promise<void> {
  const d = await open();
  const tx = d.transaction('meta', 'readwrite');
  for (const m of list) tx.objectStore('meta').put(m);
  await done(tx);
  for (const m of list) metas.set(m.id, m);
}

async function handle(r: IoRequest): Promise<{ value: unknown; transfer?: Transferable[] }> {
  await ready();
  const now = Date.now();
  switch (r.op) {
    case 'list': {
      const list = sectionList(metas.values(), r.section, r.query);
      return { value: { total: list.length, rows: windowOf(list, r.start, r.count) } };
    }
    case 'get':
      return { value: metas.get(r.file) ?? null };
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
      };
      const d = await open();
      const tx = d.transaction(['meta', 'bytes'], 'readwrite');
      tx.objectStore('meta').put(m);
      if (bytes) tx.objectStore('bytes').put(bytes, m.id);
      await done(tx);
      metas.set(m.id, m);
      changed();
      return { value: m };
    }
    case 'read': {
      const m = meta(r.file);
      const d = await open();
      const bytes = (await request(d.transaction('bytes').objectStore('bytes').get(r.file))) as unknown;
      const buf = bytes instanceof ArrayBuffer ? bytes : null;
      return { value: { meta: m, bytes: buf }, transfer: buf ? [buf] : [] };
    }
    case 'write': {
      const m = meta(r.file);
      if (failWrites) throw new Error('недостаточно места в хранилище браузера');
      const next: FileMeta = {
        ...m,
        name: cleanName(r.name),
        editedAt: now,
        size: r.bytes.byteLength,
        thumbVersion: m.thumbVersion + 1,
      };
      const d = await open();
      const tx = d.transaction(['meta', 'bytes', 'thumbs'], 'readwrite');
      tx.objectStore('meta').put(next);
      tx.objectStore('bytes').put(r.bytes, m.id);
      if (r.thumb) tx.objectStore('thumbs').put(new Blob([r.thumb], { type: 'image/png' }), m.id);
      else tx.objectStore('thumbs').delete(m.id);
      await done(tx);
      metas.set(m.id, next);
      changed();
      return { value: next };
    }
    case 'rename': {
      const m = meta(r.file);
      const name = cleanName(r.name);
      if (name === m.name) return { value: m };
      const next = { ...m, name, editedAt: now };
      await putMeta(next);
      changed();
      return { value: next };
    }
    case 'touch': {
      const next = { ...meta(r.file), openedAt: now };
      await putMeta(next);
      changed();
      return { value: next };
    }
    case 'trash':
    case 'restore': {
      const next = { ...meta(r.file) };
      if (r.op === 'trash') next.trashedAt = now;
      else delete next.trashedAt;
      await putMeta(next);
      changed();
      return { value: null };
    }
    case 'delete':
    case 'emptyTrash': {
      const ids = r.op === 'delete' ? [meta(r.file).id] : [...metas.values()].filter((m) => m.trashedAt !== undefined).map((m) => m.id);
      const d = await open();
      const tx = d.transaction(['meta', 'bytes', 'thumbs'], 'readwrite');
      for (const id of ids) for (const s of ['meta', 'bytes', 'thumbs']) tx.objectStore(s).delete(id);
      await done(tx);
      for (const id of ids) metas.delete(id);
      changed();
      return { value: null };
    }
    case 'thumb': {
      const m = metas.get(r.file);
      if (!m || m.thumbVersion === 0) return { value: null };
      const d = await open();
      const blob = (await request(d.transaction('thumbs').objectStore('thumbs').get(r.file))) as unknown;
      return { value: blob instanceof Blob ? blob : null };
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
        });
      }
      await putMeta(...list);
      changed();
      return { value: null };
    }
    case 'failWrites':
      if (!test) throw new Error('Только в тестовом режиме');
      failWrites = r.on;
      return { value: null };
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
