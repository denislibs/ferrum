// Комментарии в `sync.worker`: порядок по `seq`, догоняние без дублей, непрочитанное, открытое обсуждение и метки ядру. Сервер —
// свой `fetch` с теми же видами ответов, что у настоящего (ответы проходят строгий разбор протокола), часы ручные.

import { expect, test } from '@playwright/test';
import type { CommentDto, CommentEvent, ThreadDto, ThreadView } from '@ferrum/protocol';
import { CommentStore, initialsOf, type CommentsCoreCommand, type CommentsToMain } from '../src/core/comments-store';

const ME = 'me';
const OTHER = 'other';
const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;

function comment(thread: number, n: number, author = OTHER, body = `текст ${n}`): CommentDto {
  return {
    id: uuid(1000 + n),
    thread: uuid(thread),
    author: { id: author, name: author === ME ? 'Я Сам' : 'Иван Петров' },
    body,
    createdAt: n,
    editedAt: null,
    mentions: {},
  };
}

function thread(n: number, seq: number, extra: Partial<ThreadView> = {}): ThreadView {
  const first = comment(n, n * 100, extra.author?.id ?? OTHER);
  return {
    id: uuid(n),
    pin: { page: '1@1', node: null, x: n, y: n, wx: n, wy: n },
    author: first.author,
    createdAt: 1,
    resolvedAt: null,
    resolvedBy: null,
    seq,
    first,
    replies: 0,
    lastAt: 1,
    participants: [first.author.id!],
    unread: false,
    ...extra,
  };
}

function dto(t: ThreadView): ThreadDto {
  const { unread, ...rest } = t;
  void unread;
  return rest;
}

class Clock {
  private timers = new Map<number, () => void>();
  private next = 0;
  now = () => 0;
  setTimeout = (fn: () => void) => {
    const id = ++this.next;
    this.timers.set(id, fn);
    return id;
  };
  clearTimeout = (t: unknown) => void this.timers.delete(t as number);
  run(): void {
    const all = [...this.timers.values()];
    this.timers.clear();
    for (const fn of all) fn();
  }
}

interface Call {
  path: string;
  method: string;
  body: unknown;
}

function stand() {
  const clock = new Clock();
  const calls: Call[] = [];
  const core: CommentsCoreCommand[] = [];
  const main: CommentsToMain[] = [];
  /** Ответы по адресу: функция от запроса, очередь страниц списка. */
  const routes: ((c: Call) => { status: number; body: unknown } | null)[] = [];
  const gates: Promise<void>[] = [];
  const store = new CommentStore({
    user: ME,
    base: '/api/files/f',
    clock,
    core: (c) => core.push(c),
    post: (m) => main.push(m),
    fetch: async (path, init) => {
      const call = { path, method: init?.method ?? 'GET', body: init?.body ? JSON.parse(init.body as string) : undefined };
      calls.push(call);
      const gate = gates.shift();
      if (gate) await gate;
      for (const r of routes) {
        const hit = r(call);
        if (hit) return new Response(JSON.stringify(hit.body), { status: hit.status });
      }
      return new Response(JSON.stringify({ error: 'NOT_FOUND' }), { status: 404 });
    },
  });
  const settle = () => new Promise((r) => setTimeout(r, 0));
  const list = (after: number, body: { seq: number; threads: ThreadView[]; removed?: string[]; more?: boolean }) =>
    routes.push((c) =>
      c.path === `/api/files/f/comments?after=${after}&limit=500` ? { status: 200, body: { removed: [], more: false, ...body } } : null,
    );
  return { store, clock, calls, core, main, routes, gates, settle, list };
}

const ev = (
  seq: number,
  t: ThreadView,
  action: 'create' | 'reply' | 'edit' = 'reply',
  by = OTHER,
  c: CommentDto | null = null,
): CommentEvent => ({
  kind: 'thread',
  seq,
  by,
  action,
  thread: dto({ ...t, seq }),
  comment: c,
  deleted: null,
});

test('первая загрузка — страницами, ядру метки одним набором, панели — списком целиком', async () => {
  const s = stand();
  s.list(0, { seq: 2, threads: [thread(1, 1), thread(2, 2)], more: true });
  s.list(2, { seq: 5, threads: [thread(3, 3)] });
  s.store.connected('editor');
  await s.settle();
  expect(s.core.filter((c) => c.type === 'commentPins')).toEqual([
    expect.objectContaining({
      type: 'commentPins',
      replace: true,
      pins: expect.arrayContaining([expect.objectContaining({ id: uuid(3) })]),
    }),
  ]);
  expect(s.core[0]).toEqual({ type: 'commentsAvailable', on: true });
  s.clock.run();
  const out = s.main.filter((m) => m.t === 'threads');
  expect(out).toHaveLength(1);
  expect(out[0]).toMatchObject({ reset: true });
  expect((out[0] as { upserts: ThreadView[] }).upserts.map((t) => t.id).sort()).toEqual([uuid(1), uuid(2), uuid(3)]);
  expect(s.store.seq).toBe(5);
});

test('события: следующее применяется, старое пропускается, дыра — догоняние без дублей', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.store.event(ev(2, thread(1, 1), 'reply'));
  expect(s.store.thread(uuid(1))?.seq).toBe(2);
  // Старое (уже учтено) — пропускается.
  s.store.event(ev(2, { ...thread(1, 1), replies: 99 }, 'reply'));
  expect(s.store.thread(uuid(1))?.replies).toBe(0);
  // Старое событие обсуждения, которого ещё нет у клиента, тоже пропускается: известный номер не откатывается назад.
  s.store.event(ev(1, thread(7, 1), 'create'));
  expect(s.store.thread(uuid(7))).toBeUndefined();
  expect(s.store.seq).toBe(2);
  // Дыра: 3 потеряно, пришло 4 — догоняние `after=2`; 4 придёт и списком, и ждущим событием, а применится один раз.
  s.list(2, { seq: 4, threads: [thread(2, 3), { ...thread(1, 4), replies: 1 }] });
  s.store.event(ev(4, { ...thread(1, 4), replies: 1 }));
  await s.settle();
  expect(s.calls.map((c) => c.path)).toContain('/api/files/f/comments?after=2&limit=500');
  expect(s.store.seq).toBe(4);
  expect(s.store.thread(uuid(2))).toBeTruthy();
  expect(s.store.thread(uuid(1))?.replies).toBe(1);
});

test('gap и новое соединение — догоняние от известного, а не весь список заново', async () => {
  const s = stand();
  s.list(0, { seq: 3, threads: [thread(1, 3)] });
  s.store.connected('viewer');
  await s.settle();
  s.list(3, { seq: 6, threads: [thread(2, 6)], removed: [uuid(1)] });
  s.store.event({ kind: 'gap', seq: 6 });
  await s.settle();
  expect(s.store.thread(uuid(1))).toBeUndefined();
  expect(s.core).toContainEqual({ type: 'commentPinsRemove', ids: [uuid(1)] });
  s.list(6, { seq: 6, threads: [] });
  s.store.connected('viewer');
  await s.settle();
  expect(s.calls.filter((c) => c.path.includes('after=0'))).toHaveLength(1);
  expect(s.calls.at(-1)!.path).toBe('/api/files/f/comments?after=6&limit=500');
});

test('события во время догоняния ждут и применяются после, по порядку', async () => {
  const s = stand();
  let open!: () => void;
  s.gates.push(new Promise<void>((r) => (open = r)));
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  s.store.event(ev(3, { ...thread(1, 1), replies: 2 }));
  s.store.event(ev(2, { ...thread(1, 1), replies: 1 }));
  open();
  await s.settle();
  expect(s.store.seq).toBe(3);
  expect(s.store.thread(uuid(1))?.replies).toBe(2);
  // Догоняния лишнего нет: 2 и 3 — подряд после 1.
  expect(s.calls.filter((c) => c.path.includes('comments?after'))).toHaveLength(1);
});

test('непрочитанное: чужой ответ — да, свой — нет, в открытом обсуждении — нет и отмечается прочитанным', async () => {
  const s = stand();
  s.list(0, { seq: 2, threads: [thread(1, 1), thread(2, 2)] });
  s.store.connected('editor');
  await s.settle();
  s.store.event(ev(3, thread(1, 1), 'reply', OTHER));
  expect(s.store.thread(uuid(1))?.unread).toBe(true);
  s.store.event(ev(4, thread(1, 1), 'reply', ME));
  expect(s.store.thread(uuid(1))?.unread).toBe(false);
  // Правка чужого не делает обсуждение новым.
  s.store.event(ev(5, thread(1, 1), 'edit', OTHER));
  expect(s.store.thread(uuid(1))?.unread).toBe(false);
  // Открытое обсуждение: подробности, затем чужой ответ — сразу в окно, прочитано, на сервер — `read`.
  s.routes.push((c) =>
    c.path === `/api/files/f/comments/${uuid(2)}`
      ? { status: 200, body: { thread: { ...thread(2, 2), unread: true }, comments: [comment(2, 200)] } }
      : null,
  );
  s.routes.push((c) => (c.path.endsWith('/read') ? { status: 200, body: { ok: true } } : null));
  await s.store.request(1, { k: 'open', thread: uuid(2) });
  expect(s.calls.some((c) => c.path === `/api/files/f/comments/${uuid(2)}/read` && c.method === 'POST')).toBe(true);
  const reply = comment(2, 201);
  s.store.event(ev(6, thread(2, 2), 'reply', OTHER, reply));
  await s.settle();
  expect(s.store.thread(uuid(2))?.unread).toBe(false);
  const detail = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(detail.comments.map((c) => c.id)).toEqual([comment(2, 200).id, reply.id]);
  expect(s.calls.filter((c) => c.path.endsWith(`${uuid(2)}/read`))).toHaveLength(2);
});

test('удалённое обсуждение закрывает окно; удалённый комментарий уходит из окна', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.clock.run();
  s.routes.push((c) =>
    c.path === `/api/files/f/comments/${uuid(1)}`
      ? { status: 200, body: { thread: thread(1, 1), comments: [comment(1, 100), comment(1, 101)] } }
      : null,
  );
  await s.store.request(1, { k: 'open', thread: uuid(1) });
  s.store.event({
    kind: 'thread',
    seq: 2,
    by: OTHER,
    action: 'delete',
    thread: dto(thread(1, 2)),
    comment: null,
    deleted: comment(1, 101).id,
  });
  let d = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(d.comments.map((c) => c.id)).toEqual([comment(1, 100).id]);
  s.store.event({ kind: 'removed', seq: 3, by: OTHER, thread: uuid(1) });
  d = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(d).toEqual({ t: 'detail', thread: uuid(1), view: null, comments: [] });
  s.clock.run();
  expect(s.main.filter((m) => m.t === 'threads').at(-1)).toMatchObject({ removed: [uuid(1)] });
});

test('двигать метку: автор и владелец; отказ сервера возвращает метку на место', async () => {
  const s = stand();
  const mine = thread(1, 1, { author: { id: ME, name: 'Я Сам' } });
  s.list(0, { seq: 2, threads: [mine, thread(2, 2)] });
  s.store.connected('editor');
  await s.settle();
  const pins = (s.core.find((c) => c.type === 'commentPins') as Extract<CommentsCoreCommand, { type: 'commentPins' }>).pins;
  expect(pins.find((p) => p.id === uuid(1))?.movable).toBe(true);
  expect(pins.find((p) => p.id === uuid(2))?.movable).toBe(false);
  s.list(2, { seq: 2, threads: [] });
  s.store.connected('owner');
  await s.settle();
  const again = (s.core.filter((c) => c.type === 'commentPins').at(-1) as Extract<CommentsCoreCommand, { type: 'commentPins' }>).pins;
  expect(again.every((p) => p.movable)).toBe(true);
  s.routes.unshift((c) => (c.method === 'PATCH' ? { status: 403, body: { error: 'FORBIDDEN' } } : null));
  const before = s.core.length;
  await s.store.request(2, { k: 'move', thread: uuid(2), pin: { page: '1@1', node: null, x: 50, y: 50, wx: 50, wy: 50 } });
  expect(s.main.at(-1)).toEqual({ t: 'result', id: 2, result: { ok: false, failure: 'FORBIDDEN' } });
  expect(s.core.slice(before)).toEqual([
    { type: 'commentPins', replace: false, pins: [expect.objectContaining({ id: uuid(2), x: 2, wx: 2 })] },
  ]);
});

test('панель получает изменения пачкой, не по событию', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.clock.run();
  const before = s.main.filter((m) => m.t === 'threads').length;
  for (let i = 2; i < 12; i++) s.store.event(ev(i, thread(1, 1), 'edit'));
  expect(s.main.filter((m) => m.t === 'threads')).toHaveLength(before);
  s.clock.run();
  const out = s.main.filter((m) => m.t === 'threads');
  expect(out).toHaveLength(before + 1);
  expect(out.at(-1)).toMatchObject({ reset: false, upserts: [expect.objectContaining({ seq: 11 })] });
});

test('сбой сети при загрузке — повтор по таймеру', async () => {
  const s = stand();
  s.routes.push((c) => (c.path.includes('after=0') && s.calls.length === 1 ? { status: 503, body: { error: 'UNAVAILABLE' } } : null));
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  expect(s.store.thread(uuid(1))).toBeUndefined();
  s.clock.run();
  await s.settle();
  expect(s.store.thread(uuid(1))).toBeTruthy();
});

test('инициалы — по первым буквам двух слов, удалённый — «?»', () => {
  expect(initialsOf({ id: 'a', name: 'иван петров сидоров' })).toBe('ИП');
  expect(initialsOf({ id: 'a', name: '  ' })).toBe('?');
  expect(initialsOf({ id: null, name: 'Иван' })).toBe('?');
});

test('первый список уходит панели только целиком: вторая страница ещё в пути — панели ничего', async () => {
  const s = stand();
  let open!: () => void;
  s.list(0, { seq: 1, threads: [thread(1, 1)], more: true });
  s.list(1, { seq: 2, threads: [thread(2, 2)] });
  s.gates.push(Promise.resolve(), new Promise<void>((r) => (open = r)));
  s.store.connected('editor');
  await s.settle();
  s.clock.run();
  expect(s.main.filter((m) => m.t === 'threads')).toEqual([]);
  open();
  await s.settle();
  s.clock.run();
  const out = s.main.filter((m) => m.t === 'threads');
  expect(out).toHaveLength(1);
  expect((out[0] as { upserts: ThreadView[] }).upserts.map((t) => t.id).sort()).toEqual([uuid(1), uuid(2)]);
});

/** Загружен список до `seq` 1 с обсуждением 1; следующий запрос задержан до `open()`. */
async function loadedAndHeld() {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  let open!: () => void;
  s.gates.push(new Promise<void>((r) => (open = r)));
  return { s, open: () => open() };
}

const listCalls = (s: ReturnType<typeof stand>) => s.calls.filter((c) => c.path.includes('comments?after')).map((c) => c.path);

test('T9-I1: событие во время догоняния после загрузки ждёт его конца — событие за дырой не теряется', async () => {
  const { s, open } = await loadedAndHeld();
  s.list(1, { seq: 2, threads: [thread(1, 2)] });
  s.store.event({ kind: 'gap', seq: 2 });
  // Список отдаст до 2, а 3 пришло, пока догоняние шло.
  s.store.event(ev(3, { ...thread(1, 1), replies: 3 }));
  open();
  await s.settle();
  expect(s.store.seq).toBe(3);
  expect(s.store.thread(uuid(1))?.replies).toBe(3);
  expect(listCalls(s)).toHaveLength(2);
});

test('T9-I2: новое соединение во время догоняния — ещё один проход от известного', async () => {
  const { s, open } = await loadedAndHeld();
  s.list(1, { seq: 2, threads: [thread(1, 2)] });
  s.store.event({ kind: 'gap', seq: 2 });
  // Запрос `after=1` ушёл до подтверждения нового соединения: то, что сервер записал между ними, этому соединению не придёт.
  s.store.connected('editor');
  s.list(2, { seq: 3, threads: [thread(2, 3)] });
  open();
  await s.settle();
  expect(listCalls(s)).toEqual([
    '/api/files/f/comments?after=0&limit=500',
    '/api/files/f/comments?after=1&limit=500',
    '/api/files/f/comments?after=2&limit=500',
  ]);
  expect(s.store.seq).toBe(3);
  expect(s.store.thread(uuid(2))).toBeTruthy();
});

test('T9-M1: ответ, пришедший пока грузилось окно, не теряется — окно грузится ещё раз', async () => {
  const { s, open } = await loadedAndHeld();
  const reply = comment(1, 101);
  let n = 0;
  s.routes.push((c) => {
    if (c.path !== `/api/files/f/comments/${uuid(1)}`) return null;
    // Первый ответ сервер собрал до ответа 101, второй — после.
    return ++n === 1
      ? { status: 200, body: { thread: thread(1, 1), comments: [comment(1, 100)] } }
      : { status: 200, body: { thread: { ...thread(1, 2), replies: 1 }, comments: [comment(1, 100), reply] } };
  });
  const opening = s.store.request(1, { k: 'open', thread: uuid(1) });
  s.store.event(ev(2, { ...thread(1, 1), replies: 1 }, 'reply', OTHER, reply));
  open();
  await opening;
  const d = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(d.comments.map((c) => c.id)).toEqual([comment(1, 100).id, reply.id]);
  expect(d.view?.seq).toBe(2);
});

test('T9-M2: `gap`, дождавшийся конца догоняния и уже учтённый, второго догоняния не запускает', async () => {
  const { s, open } = await loadedAndHeld();
  s.list(1, { seq: 3, threads: [thread(1, 3)] });
  // Дыра: 2 потеряно — догоняние `after=1`; пока оно идёт, сервер присылает `gap` с номером, который список уже покроет.
  s.store.event(ev(3, thread(1, 1)));
  s.store.event({ kind: 'gap', seq: 3 });
  open();
  await s.settle();
  expect(s.store.seq).toBe(3);
  expect(listCalls(s)).toHaveLength(2);
});

test('T9-M3: догоняние после загрузки шлёт ядру метки страницы одной пачкой', async () => {
  const { s, open } = await loadedAndHeld();
  s.list(1, { seq: 4, threads: [thread(2, 2), thread(3, 3), thread(4, 4)], removed: [uuid(1)] });
  const before = s.core.length;
  s.store.event({ kind: 'gap', seq: 4 });
  open();
  await s.settle();
  expect(s.core.slice(before)).toEqual([
    {
      type: 'commentPins',
      replace: false,
      pins: [expect.objectContaining({ id: uuid(2) }), expect.objectContaining({ id: uuid(3) }), expect.objectContaining({ id: uuid(4) })],
    },
    { type: 'commentPinsRemove', ids: [uuid(1)] },
  ]);
});

test('T9-M4: событие с удалённым и новым комментарием меняет окно целиком', async () => {
  const s = stand();
  s.list(0, { seq: 1, threads: [thread(1, 1)] });
  s.store.connected('editor');
  await s.settle();
  s.routes.push((c) =>
    c.path === `/api/files/f/comments/${uuid(1)}`
      ? { status: 200, body: { thread: thread(1, 1), comments: [comment(1, 100), comment(1, 101)] } }
      : null,
  );
  await s.store.request(1, { k: 'open', thread: uuid(1) });
  const added = comment(1, 102);
  s.store.event({ ...ev(2, thread(1, 1), 'edit', OTHER, added), deleted: comment(1, 101).id } as CommentEvent);
  const d = s.main.filter((m) => m.t === 'detail').at(-1) as Extract<CommentsToMain, { t: 'detail' }>;
  expect(d.comments.map((c) => c.id)).toEqual([comment(1, 100).id, added.id]);
});
