// Worker файлов сервера: всё, что размером с документ, идёт здесь. Журнал правок — своя база на каждого
// человека (B-190): вошедший следующим в этом браузере чужих правок не увидит. Опрос доступа открытых
// файлов идёт только пока вкладка видна.

import { MAX_JOURNAL_ENTRIES } from '../core/limits';
import { ACCESS_POLL_MS, asUser, ServerFiles, type JournalEntries, type JournalStore } from '../core/server-files';
import type { FromServer, ServerRequest, ToServer } from '../core/server-messages';

const scope = self as unknown as DedicatedWorkerGlobalScope;
const post = (m: FromServer, transfer: Transferable[] = []) => scope.postMessage(m, transfer);

function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error('ошибка хранилища браузера'));
  });
}

function journalStore(user: string): JournalStore {
  let db: Promise<IDBDatabase> | null = null;
  const open = () => {
    db ??= new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open(`ferrum-user-${user}`, 1);
      req.onupgradeneeded = () => req.result.createObjectStore('journal');
      req.onsuccess = () => {
        req.result.onversionchange = () => {
          req.result.close();
          db = null;
        };
        resolve(req.result);
      };
      req.onerror = () => reject(req.error ?? new Error('хранилище браузера недоступно'));
    });
    db.catch(() => (db = null));
    return db;
  };
  const range = (file: string) => IDBKeyRange.bound([file], [file, []]);
  const finished = (tx: IDBTransaction) =>
    new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = tx.onabort = () => reject(tx.error ?? new Error('ошибка хранилища браузера'));
    });
  return {
    async append(file, session, batch, bytes) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      tx.objectStore('journal').put(bytes, [file, session, batch]);
      await finished(tx);
    },
    async read(file): Promise<JournalEntries> {
      const d = await open();
      const store = d.transaction('journal').objectStore('journal');
      const [count, last] = await Promise.all([
        request(store.count(range(file))),
        request(store.openKeyCursor(range(file), 'prev')).then((c) => c?.key),
      ]);
      const s = Array.isArray(last) ? last[1] : null;
      const session = typeof s === 'number' && Number.isSafeInteger(s) ? s : null;
      if (count > MAX_JOURNAL_ENTRIES) return { entries: [], session, tooLarge: true };
      const all = (await request(d.transaction('journal').objectStore('journal').getAll(range(file)))) as unknown[];
      return { entries: all.filter((b): b is ArrayBuffer => b instanceof ArrayBuffer), session, tooLarge: false };
    },
    async cut(file, session, upTo) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite', { durability: 'strict' });
      tx.objectStore('journal').delete(IDBKeyRange.bound([file], [file, session, upTo]));
      await finished(tx);
    },
    async drop(file) {
      const d = await open();
      const tx = d.transaction('journal', 'readwrite');
      tx.objectStore('journal').delete(range(file));
      await finished(tx);
    },
  };
}

const channel = new BroadcastChannel('ferrum-server-files');
let files: ServerFiles | null = null;
const watched = new Set<string>();
let visible = true;
let poll: ReturnType<typeof setInterval> | null = null;

const notifyChanged = () => {
  post({ t: 'changed' });
  channel.postMessage('changed');
};
// Другая вкладка изменила список: экрану — перечитать окно.
channel.onmessage = () => post({ t: 'changed' });

function replan(): void {
  if (poll !== null) clearInterval(poll);
  poll = null;
  if (!visible) return;
  poll = setInterval(() => {
    // Открытые файлы — проверка доступа; экран файлов — новое в списке (чужие приглашения и правки).
    if (watched.size === 0) post({ t: 'changed' });
    for (const f of watched) void files?.checkAccess(f);
  }, ACCESS_POLL_MS);
}

async function run(r: ServerRequest, f: ServerFiles): Promise<{ value: unknown; transfer?: Transferable[] }> {
  switch (r.op) {
    case 'list':
      return { value: await f.list(r.section, r.query, r.start, r.count) };
    case 'get':
      return { value: await f.get(r.file) };
    case 'create':
      return { value: await f.create(r.name, r.bytes, r.project, r.journal) };
    case 'read': {
      const value = await f.read(r.file);
      return { value, transfer: [...(value.bytes ? [value.bytes] : []), ...value.journal] };
    }
    case 'write':
      return { value: await f.write(r.file, r.bytes, r.thumb, r.base, r.journal) };
    case 'appendJournal':
      await f.appendJournal(r.file, r.session, r.batch, r.bytes);
      return { value: null };
    case 'blob':
      return { value: await f.blob(r.file) };
    case 'rename':
      return { value: await f.rename(r.file, r.name) };
    case 'touch':
      return { value: await f.touch(r.file) };
    case 'trash':
      await f.trash(r.file);
      return { value: null };
    case 'restore':
      await f.restore(r.file);
      return { value: null };
    case 'delete':
      await f.remove(r.file);
      return { value: null };
    case 'emptyTrash':
      await f.emptyTrash();
      return { value: null };
    case 'thumb':
      return { value: await f.thumbnail(r.file) };
  }
}

scope.onmessage = (e: MessageEvent<ToServer>) => {
  const m = e.data;
  if (m.t === 'init') {
    files = new ServerFiles(
      asUser(m.user, (input, init) => fetch(input, init)),
      journalStore(m.user),
      {
        changed: notifyChanged,
        access: (file, state) => post({ t: 'access', file, state }),
      },
      {
        now: () => performance.now(),
        setTimeout: (fn, ms) => setTimeout(fn, ms),
        clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
      },
    );
    replan();
    return;
  }
  if (m.t === 'visible') {
    visible = m.on;
    replan();
    // Вернулись на вкладку: снятый доступ виден сразу, а не к следующему опросу.
    if (visible) for (const f of watched) void files?.checkAccess(f);
    return;
  }
  if (m.t === 'watch') {
    if (m.on) {
      watched.add(m.file);
      void files?.watchAccess(m.file);
    } else {
      watched.delete(m.file);
      files?.forgetAccess(m.file);
    }
    return;
  }
  const f = files;
  if (!f) return;
  run(m, f).then(
    ({ value, transfer }) => post({ t: 'res', id: m.id, ok: true, value }, transfer),
    (err: unknown) =>
      post({
        t: 'res',
        id: m.id,
        ok: false,
        message: err instanceof Error ? err.message : String(err),
        code: (err as { code?: string }).code,
      }),
  );
};
