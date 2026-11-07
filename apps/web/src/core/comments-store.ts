// Комментарии открытого файла со стороны клиента (спека подсистемы 4, раздел 10). Живут в `sync.worker`, рядом с сокетом файла:
// события `comment` и запросы REST сходятся здесь по `seq` файла, ядро получает метки, основной поток — только изменившиеся
// обсуждения (панель) и открытое обсуждение (окно). Логика без DOM: модульные тесты дают свой `fetch` и часы.
//
// Порядок. Событие с `seq = известный + 1` применяется; `seq ≤ известного` — уже учтено (пришло списком); дыра или `gap` —
// догоняние `GET ?after=известный` страницами. Пока идёт догоняние, события ждут и потом применяются по тому же правилу.
// Ответ своего изменения меняет обсуждение, но не `known`: его событие придёт и будет пропущено по `seq` обсуждения. Окно —
// не по `seq` обсуждения: событие соседа с меньшим номером, пришедшее после ответа своей записи, обсуждение уже не меняет, но
// его комментарий в окно попадает — по месту в порядке создания. Версия комментария в окне помнит номер изменения, с которым
// пришла: более старое событие её не откатывает и удалённый не возвращает.

import {
  COMMENT_PAGE_MAX,
  parseCommentList,
  parseCommentWrite,
  parseMentionList,
  parseThreadDetail,
  PRESENCE_COLORS,
  presenceColor,
  type CommentAuthor,
  type CommentDto,
  type CommentEvent,
  type CommentWriteDto,
  type FileRole,
  type MentionCandidate,
  type PinDto,
  type ThreadDto,
  type ThreadView,
} from '@ferrum/protocol';
import type { Clock } from './server-files';

/** Метка обсуждения для ядра (`CommentPinDto`). */
export interface CommentPinForCore {
  id: string;
  page: string;
  node?: string;
  x: number;
  y: number;
  wx: number;
  wy: number;
  initials: string;
  color: number;
  replies: number;
  unread: boolean;
  resolved: boolean;
  movable: boolean;
}

export type CommentsCoreCommand =
  | { type: 'commentsAvailable'; on: boolean }
  | { type: 'commentPins'; pins: CommentPinForCore[]; replace: boolean }
  | { type: 'commentPinsRemove'; ids: string[] };

/** Запросы основного потока. `open` — какое обсуждение открыто в окне (его живые изменения идут туда, оно не «непрочитано»). */
export type CommentRequest =
  | { k: 'open'; thread: string | null }
  | { k: 'create'; pin: PinDto; body: string }
  | { k: 'reply'; thread: string; body: string }
  | { k: 'edit'; thread: string; comment: string; body: string }
  | { k: 'delete'; thread: string; comment: string }
  | { k: 'resolve'; thread: string; resolved: boolean }
  | { k: 'move'; thread: string; pin: PinDto }
  | { k: 'mentions'; q: string };

/** Ответ на запрос: `failure` — код ошибки API, `NETWORK` или `NOT_FOUND`. */
export type CommentResult = { ok: true; value?: unknown } | { ok: false; failure: string };

export type CommentsToMain =
  /** Изменившиеся обсуждения; `reset` — список целиком заново (новый файл или первая загрузка). Больше
   * `COMMENTS_POST_BATCH` — пачками: `more` — следом идёт ещё пачка того же изменения (`reset` — только у первой). */
  | { t: 'threads'; reset: boolean; upserts: ThreadView[]; removed: string[]; more: boolean }
  /** Открытое обсуждение: все его комментарии по порядку; `null` — его больше нет (удалено или нет доступа). */
  | { t: 'detail'; thread: string; view: ThreadView | null; comments: CommentDto[] }
  | { t: 'result'; id: number; result: CommentResult }
  /** Роль в файле и кто я: права кнопок окна. */
  | { t: 'me'; user: string; role: FileRole };

export interface CommentsEnv {
  user: string;
  /** Адрес API файла: `/api/files/<id>`. */
  base: string;
  fetch(path: string, init?: RequestInit): Promise<Response>;
  core(cmd: CommentsCoreCommand): void;
  post(m: CommentsToMain): void;
  clock: Clock;
}

/** Изменения панели уходят основному потоку не чаще: поток событий не перерисовывает панель на каждое. */
export const COMMENTS_FLUSH_MS = 50;
/** Обсуждений в одном сообщении основному потоку: первая загрузка (до 10 000) — пачками, а не одним структурным копированием. */
export const COMMENTS_POST_BATCH = 500;
/** Сбой догоняния по сети — снова через столько. */
const RETRY_MS = 5000;

/** Инициалы автора для метки: первые буквы двух первых слов имени; без имени — «?». */
export function initialsOf(a: CommentAuthor): string {
  const words = a.name.trim().split(/\s+/).filter(Boolean);
  if (a.id === null || words.length === 0) return '?';
  return words
    .slice(0, 2)
    .map((w) => [...w][0] ?? '')
    .join('')
    .toLocaleUpperCase('ru');
}

/** Цвет автора — тот же, что у его курсора; удалённый — серый. */
export function authorColor(a: CommentAuthor): number {
  return a.id === null ? 0x8a8580 : parseInt(PRESENCE_COLORS[presenceColor(a.id)].slice(1), 16);
}

async function json(res: Response): Promise<unknown> {
  try {
    return (await res.json()) as unknown;
  } catch {
    return null;
  }
}

function failureOf(status: number, body: unknown): string {
  if (status >= 500) return 'UNAVAILABLE';
  const code = (body as { error?: unknown } | null)?.error;
  return typeof code === 'string' ? code : 'NETWORK';
}

export class CommentStore {
  private readonly threads = new Map<string, ThreadView>();
  private known = 0;
  private loaded = false;
  private fetching = false;
  /** Новое соединение подтверждено, пока шло догоняние: его запрос ушёл раньше, нужен ещё проход. */
  private again = false;
  private readonly waiting: CommentEvent[] = [];
  private role: FileRole | null = null;
  private open: string | null = null;
  /** Открытое обсуждение: `base` — номер, с которым оно загружено (всё до него в нём учтено), `seen` — номер последнего
   * изменения каждого комментария после загрузки (и удаления). */
  private detail: { thread: string; comments: CommentDto[]; base: number; seen: Map<string, number> } | null = null;
  /** Изменения для панели, ждущие отправки. */
  private upserts = new Map<string, ThreadView>();
  private removedOut = new Set<string>();
  private resetOut = false;
  private flushTimer: unknown = null;
  private retryTimer: unknown = null;
  private stopped = false;

  constructor(private readonly env: CommentsEnv) {}

  /** Роль подтверждена соединением (первое или новое): метки доступны, догоняем пропущенное. */
  connected(role: FileRole): void {
    if (this.stopped) return;
    const changed = this.role !== role;
    this.role = role;
    this.env.core({ type: 'commentsAvailable', on: true });
    this.env.post({ t: 'me', user: this.env.user, role });
    // Право двигать метку зависит от роли: метки заново.
    if (changed && this.loaded) this.env.core({ type: 'commentPins', pins: this.pins([...this.threads.values()]), replace: true });
    // Идущее догоняние спросило сервер до подтверждения этого соединения: изменения между ними не придут ему событиями.
    if (this.fetching) this.again = true;
    void this.catchUp();
  }

  stop(): void {
    this.stopped = true;
    if (this.flushTimer !== null) this.env.clock.clearTimeout(this.flushTimer);
    if (this.retryTimer !== null) this.env.clock.clearTimeout(this.retryTimer);
  }

  /** Событие сокета. */
  event(ev: CommentEvent): void {
    if (this.stopped) return;
    if (this.fetching || !this.loaded) {
      this.waiting.push(ev);
      return;
    }
    this.apply(ev);
  }

  private apply(ev: CommentEvent): void {
    // `gap` несёт последний номер сервера: если он уже учтён (ждал догоняния), второго догоняния не нужно.
    if (ev.kind === 'gap' ? ev.seq > this.known : ev.seq > this.known + 1) {
      void this.catchUp();
      return;
    }
    if (ev.kind === 'gap') return;
    if (ev.seq <= this.known) return;
    this.known = ev.seq;
    if (ev.kind === 'removed') {
      this.remove(ev.thread);
      return;
    }
    const was = this.threads.get(ev.thread.id);
    const fresh = ev.action === 'create' || ev.action === 'reply';
    // Обсуждение уже новее (ответ своей записи пришёл раньше события соседа): оно не меняется, а окно — да, ниже.
    if (!was || was.seq < ev.thread.seq) {
      // Непрочитано — для чужого нового комментария в обсуждении, которое не открыто; своё всегда прочитано.
      let unread = was?.unread ?? false;
      if (ev.by === this.env.user) unread = fresh ? false : unread;
      else if (fresh) unread = this.open !== ev.thread.id;
      this.upsert({ ...ev.thread, unread });
    }
    if (this.detail?.thread === ev.thread.id) {
      this.mergeDetail(ev.seq, ev.comment, ev.deleted);
      this.postDetail();
      // Открытое обсуждение читается сразу: на других вкладках и после перезагрузки оно не «новое».
      if (fresh && ev.by !== this.env.user) void this.markRead(ev.thread.id);
    }
  }

  /** Догоняние `?after=known` страницами; ждущие события — после. Первый раз — весь список. */
  private async catchUp(): Promise<void> {
    if (this.fetching || this.stopped) return;
    this.fetching = true;
    const first = !this.loaded;
    try {
      do {
        this.again = false;
        for (;;) {
          const res = await this.env.fetch(`${this.env.base}/comments?after=${this.known}&limit=${COMMENT_PAGE_MAX}`);
          const body = await json(res);
          const list = res.ok ? parseCommentList(body) : null;
          if (!list) throw new Error(`список комментариев: ${res.status}`);
          if (first && this.known === 0) this.resetOut = true;
          // Метки страницы — ядру одной пачкой: после долгого отсутствия обсуждений могут быть сотни.
          const changed: ThreadView[] = [];
          for (const t of list.threads) {
            const was = this.threads.get(t.id);
            if (!was || was.seq <= t.seq) {
              this.upsert(t, false);
              changed.push(t);
            }
          }
          const gone = list.removed.filter((id) => this.remove(id, false));
          if (this.loaded && changed.length > 0) this.env.core({ type: 'commentPins', pins: this.pins(changed), replace: false });
          if (gone.length > 0) this.env.core({ type: 'commentPinsRemove', ids: gone });
          this.known = Math.max(this.known, list.seq);
          if (!list.more) break;
        }
      } while (this.again);
      this.loaded = true;
    } catch {
      this.fetching = false;
      if (!this.stopped && this.retryTimer === null)
        this.retryTimer = this.env.clock.setTimeout(() => {
          this.retryTimer = null;
          void this.catchUp();
        }, RETRY_MS);
      return;
    }
    this.fetching = false;
    if (first) {
      this.env.core({ type: 'commentPins', pins: this.pins([...this.threads.values()]), replace: true });
      this.schedule();
    }
    const queued = this.waiting.splice(0).sort((a, b) => a.seq - b.seq);
    for (const ev of queued) {
      if (this.fetching) {
        this.waiting.push(ev);
        continue;
      }
      this.apply(ev);
    }
    // Открытое обсуждение могло измениться, пока не было связи.
    if (this.open && this.threads.has(this.open)) void this.loadDetail(this.open);
    else if (this.open) this.closeDetail(this.open);
  }

  /** `toCore: false` — метку ядру отправит вызывающий (пачкой). */
  private upsert(t: ThreadView, toCore = true): void {
    this.threads.set(t.id, t);
    this.upserts.set(t.id, t);
    this.removedOut.delete(t.id);
    if (toCore && this.loaded) this.env.core({ type: 'commentPins', pins: this.pins([t]), replace: false });
    this.schedule();
  }

  /** `false` — такого обсуждения не было. `toCore: false` — снять метку в ядре должен вызывающий. */
  private remove(id: string, toCore = true): boolean {
    if (!this.threads.delete(id)) return false;
    this.upserts.delete(id);
    this.removedOut.add(id);
    if (toCore) this.env.core({ type: 'commentPinsRemove', ids: [id] });
    if (this.open === id) this.closeDetail(id);
    this.schedule();
    return true;
  }

  private pins(list: ThreadView[]): CommentPinForCore[] {
    return list.map((t) => ({
      id: t.id,
      page: t.pin.page,
      ...(t.pin.node ? { node: t.pin.node } : {}),
      x: t.pin.x,
      y: t.pin.y,
      wx: t.pin.wx,
      wy: t.pin.wy,
      initials: initialsOf(t.author),
      color: authorColor(t.author),
      replies: t.replies,
      unread: t.unread,
      resolved: t.resolvedAt !== null,
      movable: this.role === 'owner' || (t.author.id !== null && t.author.id === this.env.user),
    }));
  }

  private schedule(): void {
    if (this.flushTimer !== null || this.stopped) return;
    this.flushTimer = this.env.clock.setTimeout(() => {
      this.flushTimer = null;
      this.flush();
    }, COMMENTS_FLUSH_MS);
  }

  private flush(): void {
    // Первый список — только целиком: по половине панель показала бы не всё, а ссылка на обсуждение со второй страницы
    // сочла бы его ненайденным. Догрузится — `catchUp` позовёт снова.
    if (!this.loaded) return;
    const reset = this.resetOut;
    const upserts = reset ? [...this.threads.values()] : [...this.upserts.values()];
    const removed = reset ? [] : [...this.removedOut];
    this.upserts.clear();
    this.removedOut.clear();
    this.resetOut = false;
    if (!reset && upserts.length === 0 && removed.length === 0) return;
    // Пачками по `COMMENTS_POST_BATCH`: `reset` и удалённые — в первой, `more` — у всех, кроме последней. Панель считает список
    // загруженным (и открывает ссылку на обсуждение) только по последней.
    for (let at = 0; at === 0 || at < upserts.length; at += COMMENTS_POST_BATCH) {
      const more = at + COMMENTS_POST_BATCH < upserts.length;
      const first = at === 0;
      this.env.post({
        t: 'threads',
        reset: reset && first,
        upserts: upserts.slice(at, at + COMMENTS_POST_BATCH),
        removed: first ? removed : [],
        more,
      });
    }
  }

  /** Обсуждение — для тестов и проб. */
  thread(id: string): ThreadView | undefined {
    return this.threads.get(id);
  }

  get seq(): number {
    return this.known;
  }

  // --- Запросы основного потока ---

  async request(id: number, req: CommentRequest): Promise<void> {
    const result = await this.run(req).catch((): CommentResult => ({ ok: false, failure: 'NETWORK' }));
    this.env.post({ t: 'result', id, result });
  }

  private send(path: string, method: string, body?: unknown): Promise<Response> {
    return this.env.fetch(`${this.env.base}${path}`, {
      method,
      ...(body === undefined ? {} : { headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }),
    });
  }

  private async write(path: string, method: string, body?: unknown): Promise<CommentResult> {
    const res = await this.send(path, method, body);
    const data = await json(res);
    if (!res.ok) return { ok: false, failure: failureOf(res.status, data) };
    const w = parseCommentWrite(data);
    if (!w) return { ok: false, failure: 'NETWORK' };
    if (w.removed) this.remove(w.thread.id);
    else {
      const was = this.threads.get(w.thread.id);
      if (!was || was.seq < w.thread.seq) this.upsert(w.thread);
      if (this.detail?.thread === w.thread.id && w.comment) {
        this.mergeDetail(w.seq, w.comment, null);
        this.postDetail();
      }
    }
    return { ok: true, value: w };
  }

  private async run(req: CommentRequest): Promise<CommentResult> {
    switch (req.k) {
      case 'open':
        this.open = req.thread;
        if (req.thread === null) {
          this.detail = null;
          return { ok: true };
        }
        return this.loadDetail(req.thread);
      case 'create':
        return this.write('/comments', 'POST', { pin: req.pin, body: req.body });
      case 'reply':
        return this.write(`/comments/${req.thread}/comments`, 'POST', { body: req.body });
      case 'edit':
        return this.write(`/comments/${req.thread}/comments/${req.comment}`, 'PATCH', { body: req.body });
      case 'delete': {
        const r = await this.write(`/comments/${req.thread}/comments/${req.comment}`, 'DELETE');
        if (r.ok && this.detail?.thread === req.thread) {
          this.mergeDetail((r.value as CommentWriteDto).seq, null, req.comment);
          if (this.threads.has(req.thread)) this.postDetail();
        }
        return r;
      }
      case 'resolve':
        return this.write(`/comments/${req.thread}`, 'PATCH', { resolved: req.resolved });
      case 'move': {
        const r = await this.write(`/comments/${req.thread}`, 'PATCH', { pin: req.pin });
        // Ядро уже поставило метку на новое место: отказ сервера возвращает её назад.
        const t = this.threads.get(req.thread);
        if (!r.ok && t) this.env.core({ type: 'commentPins', pins: this.pins([t]), replace: false });
        return r;
      }
      case 'mentions': {
        const res = await this.env.fetch(`${this.env.base}/mentions?q=${encodeURIComponent(req.q)}`);
        const data = await json(res);
        if (!res.ok) return { ok: false, failure: failureOf(res.status, data) };
        const list = parseMentionList(data);
        return list ? { ok: true, value: list.people satisfies MentionCandidate[] } : { ok: false, failure: 'NETWORK' };
      }
    }
  }

  private async loadDetail(thread: string, retried = false): Promise<CommentResult> {
    const res = await this.env.fetch(`${this.env.base}/comments/${thread}`);
    const data = await json(res);
    if (this.open !== thread) return { ok: true };
    if (!res.ok) {
      const failure = failureOf(res.status, data);
      if (failure === 'NOT_FOUND') this.closeDetail(thread);
      return { ok: false, failure };
    }
    const d = parseThreadDetail(data);
    if (!d) return { ok: false, failure: 'NETWORK' };
    const was = this.threads.get(thread);
    // Пока шёл запрос, пришёл ответ или правка (окна ещё не было — событие его не тронуло): ответ сервера устарел, ещё раз.
    // Не больше одного раза за открытие: сервер, у которого список и обсуждение расходятся, не заставит гонять запросы.
    if (was && was.seq > d.thread.seq && !retried) return this.loadDetail(thread, true);
    this.detail = { thread, comments: d.comments, base: d.thread.seq, seen: new Map() };
    // Список мог ещё не дойти (ссылка на обсуждение): обсуждение из ответа, прочитанное.
    if (!was || was.seq <= d.thread.seq) this.upsert({ ...d.thread, unread: false });
    else if (was.unread) this.upsert({ ...was, unread: false });
    this.postDetail();
    if (d.thread.unread || was?.unread) void this.markRead(thread);
    return { ok: true, value: d.thread };
  }

  /** Изменение `seq` в открытом окне. Не новее загруженного или известной версии комментария — пропускается: событие, пришедшее
   * после ответа своей записи, не откатывает правку и не возвращает удалённый. Новый комментарий встаёт в порядке сервера:
   * первый — первым, остальные по времени создания и id (чужой более ранний ответ — выше своего). */
  private mergeDetail(seq: number, comment: CommentDto | null, deleted: string | null): void {
    const d = this.detail;
    if (!d) return;
    if (deleted) {
      d.seen.set(deleted, Math.max(seq, d.seen.get(deleted) ?? 0));
      d.comments = d.comments.filter((c) => c.id !== deleted);
    }
    if (!comment || seq <= d.base || seq <= (d.seen.get(comment.id) ?? 0)) return;
    d.seen.set(comment.id, seq);
    const first = this.threads.get(d.thread)?.first.id;
    const key = (c: CommentDto): [number, number, string] => [c.id === first ? 0 : 1, c.createdAt, c.id];
    const before = (a: CommentDto, b: CommentDto): boolean => {
      const [x, y] = [key(a), key(b)];
      return x[0] !== y[0] ? x[0] < y[0] : x[1] !== y[1] ? x[1] < y[1] : x[2] < y[2];
    };
    const list = d.comments.filter((c) => c.id !== comment.id);
    const at = list.findIndex((c) => before(comment, c));
    list.splice(at < 0 ? list.length : at, 0, comment);
    d.comments = list;
  }

  private postDetail(): void {
    if (!this.detail) return;
    const view = this.threads.get(this.detail.thread) ?? null;
    this.env.post({ t: 'detail', thread: this.detail.thread, view, comments: [...this.detail.comments] });
  }

  private closeDetail(thread: string): void {
    if (this.detail?.thread === thread) this.detail = null;
    this.env.post({ t: 'detail', thread, view: null, comments: [] });
  }

  private async markRead(thread: string): Promise<void> {
    const res = await this.send(`/comments/${thread}/read`, 'POST').catch(() => null);
    if (!res?.ok) return;
    const t = this.threads.get(thread);
    if (t?.unread) this.upsert({ ...t, unread: false });
  }
}

/** Метка обсуждения — из `ThreadDto` без своего состояния (для проб и тестов). */
export const viewOf = (t: ThreadDto, unread = false): ThreadView => ({ ...t, unread });
