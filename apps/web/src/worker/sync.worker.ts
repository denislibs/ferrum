// WebSocket файла (спека подсистемы 4, раздел 7.1): всё здесь и в worker'е ядра, с ним — прямой порт, буферы передаются. Основной поток
// получает только состояние соединения и роль.

import { asUser, ServerFiles, type JournalStore } from '../core/server-files';
import { CommentStore } from '../core/comments-store';
import { SyncClient, type CoreCommand, type SocketLike } from '../core/sync-client';
import type { CoreToSync, FromSync, SyncToCore, ToSync } from '../core/sync-messages';

const scope = self as unknown as DedicatedWorkerGlobalScope;
const post = (m: FromSync) => scope.postMessage(m);

/** Журнала здесь нет: им занят worker файлов, здесь только запросы картинок и снимков. */
const noJournal: JournalStore = {
  append: () => Promise.reject(new Error('нет журнала')),
  read: () => Promise.resolve({ entries: [], session: null, tooLarge: false }),
  cut: () => Promise.resolve(),
  cutExact: () => Promise.resolve(),
  drop: () => Promise.resolve(),
};

let client: SyncClient | null = null;
let comments: CommentStore | null = null;

function transferOf(c: CoreCommand): Transferable[] {
  switch (c.type) {
    case 'syncSubscribed':
      return [c.version.buffer as ArrayBuffer];
    case 'applyRemote':
      return c.batches.map((b) => b.buffer as ArrayBuffer);
    case 'addRemoteImage':
      return [c.bytes.buffer as ArrayBuffer];
    default:
      return [];
  }
}

scope.onmessage = (e: MessageEvent<ToSync>) => {
  const m = e.data;
  if (m.t === 'stop') {
    client?.stop();
    comments?.stop();
    client = comments = null;
    return;
  }
  if (m.t === 'comments') {
    if (comments) void comments.request(m.id, m.req);
    else post({ t: 'comments', m: { t: 'result', id: m.id, result: { ok: false, failure: 'NETWORK' } } });
    return;
  }
  const port = m.port;
  const toCore = (s: SyncToCore, transfer: Transferable[] = []) => port.postMessage(s, transfer);
  const files = new ServerFiles(
    asUser(m.user, (input, init) => fetch(input, init)),
    noJournal,
    { changed: () => {}, access: () => {} },
    {
      now: () => performance.now(),
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
    },
  );
  const clock = {
    now: () => performance.now(),
    setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
    clearTimeout: (t: unknown) => clearTimeout(t as ReturnType<typeof setTimeout>),
  };
  const fetchAs = asUser(m.user, (input, init) => fetch(input, init));
  comments = new CommentStore({
    user: m.user,
    base: `/api/files/${m.file}`,
    fetch: (path, init) => fetchAs(path, init),
    core: (cmd) => toCore({ t: 'cmd', cmd }),
    post: (c) => post({ t: 'comments', m: c }),
    clock,
  });
  const store = comments;
  client = new SyncClient({
    file: m.file,
    user: m.user,
    schema: m.schema,
    origin: self.location.origin,
    socket: (url) => new WebSocket(url) as unknown as SocketLike,
    core: {
      command: (cmd) => toCore({ t: 'cmd', cmd }, transferOf(cmd)),
      flush: (on) => toCore({ t: 'flush', on }),
    },
    files: {
      uploadImage: (hash, bytes) => files.uploadOneImage(m.file, hash, bytes),
      downloadImage: (hash) => files.downloadOneImage(m.file, hash),
      snapshot: (path) => files.fetchSnapshot(path),
      access: () => files.accessOnce(m.file),
    },
    clock: {
      now: () => performance.now(),
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
    },
    random: Math.random,
    status: (status, detail) => post({ t: 'status', status, detail }),
    access: (state) => post({ t: 'access', state }),
    reload: () => post({ t: 'reload' }),
    participants: (list) => post({ t: 'participants', list }),
    comments: { connected: (role) => store.connected(role), event: (ev) => store.event(ev) },
  });
  port.onmessage = (ev: MessageEvent<CoreToSync>) => {
    if (ev.data.t === 'applied') client?.onApplied();
    else if (ev.data.t === 'reset') client?.onReset();
    else client?.onCore(ev.data.event);
  };
  client.start();
};

scope.addEventListener('online', () => client?.online());
