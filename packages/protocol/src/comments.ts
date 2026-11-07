// Комментарии (спека подсистемы 4, раздел 10): пределы, текст, виды запросов и ответов REST и событие `comment` по
// WebSocket файла. Комментарии живут в Postgres, а не в Loro: не шаг отмены, не часть документа, не экспорт. Разбор не
// доверяет данным: неверное — `null`, лишние поля тоже неверны (событие приходит из сети, запрос — от чужого клиента).

import { INVISIBLE_BUT_ZWJ_RE, MAX_NAME_CHARS } from './auth';
import { isUuid } from './files';
import { isPageDoc } from './presence';

/** Длина текста — в кодовых точках (`commentLength`): одна функция у клиента и сервера, иначе поле ввода и сервер
 * спорили бы о смайликах. */
export const COMMENT_BODY_MAX = 4000;
/** Разных упоминаний в одном комментарии: больше — 400. Каждое — строка базы и, может быть, письмо. */
export const COMMENT_MENTIONS_MAX = 20;
/** Обсуждений в одной странице списка: больше `limit` урезается до этого. */
export const COMMENT_PAGE_MAX = 500;
/** Живых обсуждений в файле — как `COMMENT_PINS_MAX` ядра: новое сверх — 409 `FILE_FULL`. Иначе любой с доступом, и читатель по
 * ссылке, набил бы файл так, что каждое открытие грузило бы сотни мегабайт. */
export const COMMENT_THREADS_MAX = 10_000;
/** Живых обсуждений одного человека в файле: сверх — 409 `AUTHOR_FULL`. Один человек не занимает весь предел файла. */
export const COMMENT_THREADS_PER_USER_MAX = 1000;
/** Комментариев в обсуждении: ответ сверх — 409 `THREAD_FULL`. Обсуждение отдаётся целиком одним ответом. */
export const THREAD_COMMENTS_MAX = 500;
/** Байт текста (UTF-8) всех комментариев обсуждения: ответ или правка сверх — 409 `THREAD_FULL`. Обсуждение отдаётся целиком
 * одним ответом (окно показывает его всё), поэтому предел — у объёма, а не страницы: 500 комментариев по 1 КБ помещаются,
 * 500 предельных по 16 КБ (8 МБ на каждое открытие окна) — нет. */
export const THREAD_BYTES_MAX = 512 * 1024;
/** Начало текста первого комментария в списке и событии (`commentPreview`): панели больше не нужно, целиком — в окне. */
export const COMMENT_PREVIEW_MAX = 280;
/** Подсказка упоминаний: столько людей и строка поиска не длиннее `MENTION_QUERY_MAX`. */
export const MENTION_SUGGEST_MAX = 8;
export const MENTION_QUERY_MAX = 64;
/** Участников обсуждения в `ThreadDto`: для фильтра «Только мои» и писем больше не нужно. */
export const THREAD_PARTICIPANTS_MAX = 50;
/** Событие `comment` на проводе — JSON не больше этого: два текста по 4000 кодовых точек, упоминания и участники с запасом
 * помещаются (`comments.test.ts` проверяет худший случай). */
export const COMMENT_EVENT_MAX_BYTES = 64 * 1024;
/** Координаты метки: как у присутствия, дальше мира ядра не бывает. */
const COORD_MAX = 1e9;

const USER_ID_RE = /^[A-Za-z0-9_-]{1,64}$/;
/** Упоминание в тексте: `@[userId]`. Имя не хранится — оно показывается по текущему имени человека. */
const MENTION_RE = /@\[([A-Za-z0-9_-]{1,64})\]/g;
/** Управляющие C0 (кроме перевода строки и табуляции), DEL и C1. */
// eslint-disable-next-line no-control-regex
const CONTROLS_RE = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g;
/** Соединитель нулевой ширины не внутри эмодзи (👩‍💻 держится им, а сам по себе он невидим). */
const LOOSE_ZWJ_RE = /(?<!\p{Extended_Pictographic}(?:\u{fe0f}|\p{Emoji_Modifier})?)\u200d|\u200d(?!\p{Extended_Pictographic})/gu;
/** Комбинирующих знаков подряд (на одну графему) — не больше 8: сверх них строка «наползает» на соседние в окне и панели. */
const MARKS_RE = /(\p{M}{8})\p{M}+/gu;
/** Видимое: не пробел, не формат, не комбинирующий знак сам по себе. */
const VISIBLE_RE = /[^\s\p{Cf}\p{Cc}\p{Mn}\p{Me}]/u;

export const isCommentUserId = (v: unknown): v is string => typeof v === 'string' && USER_ID_RE.test(v);

/** Длина в кодовых точках: `[...s].length`, а не `s.length` в UTF-16. */
export function commentLength(s: string): number {
  let n = 0;
  for (const _ of s) n++;
  return n;
}

/** Очистка текста без проверок: переводы строк — `\n`, управляющие, знаки направления и невидимые (тот же класс, что у имён,
 * `auth.ts`; ZWJ — только вне эмодзи) убираются, комбинирующих знаков подряд — не больше 8, одиночные суррогаты заменяются,
 * края обрезаются. Повторная очистка ничего не меняет. */
function cleanCommentText(s: string): string {
  return s
    .toWellFormed()
    .replace(/\r\n?/g, '\n')
    .replace(CONTROLS_RE, '')
    .replace(INVISIBLE_BUT_ZWJ_RE, '')
    .replace(LOOSE_ZWJ_RE, '')
    .replace(MARKS_RE, '$1')
    .trim();
}

/** Текст комментария: только текст (`cleanCommentText`) и хотя бы один видимый знак — комментарий из невидимых выглядел бы
 * пустым. Пусто или длиннее `COMMENT_BODY_MAX` — `null`. Сервер хранит уже нормальный текст: клиент показывает его как есть,
 * без HTML. Одна функция у поля ввода и сервера. */
export function normalizeCommentBody(s: unknown): string | null {
  if (typeof s !== 'string') return null;
  // Предел до разбора: строка из запроса уже не больше JSON, но разбор чужого мегабайта не нужен и клиенту.
  if (s.length > COMMENT_BODY_MAX * 4) return null;
  const body = cleanCommentText(s);
  return VISIBLE_RE.test(body) && commentLength(body) <= COMMENT_BODY_MAX ? body : null;
}

/** Начало нормального текста для списка и события: не больше `COMMENT_PREVIEW_MAX` кодовых точек, упоминание на краю не
 * режется пополам, края обрезаны; `more` — текст длиннее. Сервер считает его при записи, клиент только показывает. */
export function commentPreview(body: string): { text: string; more: boolean } {
  if (commentLength(body) <= COMMENT_PREVIEW_MAX) return { text: body, more: false };
  let text = '';
  let n = 0;
  for (const ch of body) {
    if (++n > COMMENT_PREVIEW_MAX) break;
    text += ch;
  }
  // Упоминание без закрывающей скобки показалось бы текстом `@[id`: оно уходит целиком.
  const at = text.lastIndexOf('@[');
  if (at >= 0 && !/^@\[[A-Za-z0-9_-]{1,64}\]/.test(text.slice(at))) text = text.slice(0, at);
  // Край мог отрезать эмодзи после соединителя: висящий ZWJ очистка убрала бы, начало должно быть уже чистым.
  return { text: cleanCommentText(text), more: true };
}

/** Начало текста с провода: чистое (очистка его не меняет) и не длиннее предела. Видимость не требуется: у текста она может
 * начинаться дальше края, а неверное начало отвергло бы всю страницу списка. */
const isPreview = (v: unknown): v is string =>
  typeof v === 'string' && v.length <= COMMENT_PREVIEW_MAX * 2 && commentLength(v) <= COMMENT_PREVIEW_MAX && cleanCommentText(v) === v;

/** Люди, упомянутые в тексте: без повторов, по порядку появления. */
export function mentionIds(body: string): string[] {
  const out = new Set<string>();
  for (const m of body.matchAll(MENTION_RE)) out.add(m[1]);
  return [...out];
}

export type BodyPart = { text: string } | { mention: string; name: string | null };

/** Текст для показа: куски текста и упоминания. `names` — имена упомянутых с доступом (`CommentDto.mentions`); нет имени
 * — `name: null`, интерфейс показывает «@неизвестный»: ни имени, ни адреса человека без доступа. */
export function splitCommentBody(body: string, names: Record<string, string>): BodyPart[] {
  const out: BodyPart[] = [];
  let at = 0;
  for (const m of body.matchAll(MENTION_RE)) {
    if (m.index > at) out.push({ text: body.slice(at, m.index) });
    out.push({ mention: m[1], name: Object.hasOwn(names, m[1]) ? names[m[1]] : null });
    at = m.index + m[0].length;
  }
  if (at < body.length) out.push({ text: body.slice(at) });
  return out;
}

export interface CommentAuthor {
  /** `null` — учётная запись удалена: интерфейс пишет «Удалённый пользователь» **(поведение)**. */
  id: string | null;
  /** Пусто — «Участник», как у присутствия. */
  name: string;
}

export interface CommentDto {
  id: string;
  thread: string;
  author: CommentAuthor;
  body: string;
  createdAt: number;
  editedAt: number | null;
  /** Имена упомянутых, у кого был доступ к файлу при отправке: только им заведено упоминание (и письмо). */
  mentions: Record<string, string>;
}

/** Первый комментарий в списке, событии и ответе изменения: начало текста (`commentPreview`), `more` — текст длиннее;
 * упоминания — только те, что в начале. Целиком — в `GET …/:threadId` (окно у метки). */
export interface CommentHeadDto {
  id: string;
  author: CommentAuthor;
  body: string;
  more: boolean;
  createdAt: number;
  editedAt: number | null;
  mentions: Record<string, string>;
}

/** Метка: `page`/`node` — `TreeID` Loro (`counter@peer`); `x`,`y` — в координатах узла или мира страницы (`node: null`);
 * `wx`,`wy` — последнее мировое положение: им метка держится, когда узла нет или он скрыт **(поведение)**. */
export interface PinDto {
  page: string;
  node: string | null;
  x: number;
  y: number;
  wx: number;
  wy: number;
}

export interface ThreadDto {
  id: string;
  pin: PinDto;
  author: CommentAuthor;
  createdAt: number;
  resolvedAt: number | null;
  resolvedBy: CommentAuthor | null;
  /** Номер последнего изменения обсуждения в счётчике файла: события и `?after=` сходятся по нему. */
  seq: number;
  first: CommentHeadDto;
  replies: number;
  lastAt: number;
  /** Кто пишет в обсуждении и кого в нём упомянули, не больше `THREAD_PARTICIPANTS_MAX`. */
  participants: string[];
}

export interface ThreadView extends ThreadDto {
  /** Для того, кто спрашивает: есть чужой комментарий новее его прочтения. */
  unread: boolean;
}

/** `GET …/comments?after=N&limit=M`: обсуждения с `seq > N` по возрастанию `seq`. `after=0` — живые без надгробий;
 * иначе удалённые с тех пор — в `removed`. `more` — есть ещё: следующий запрос — `after=seq`. `seq` — номер, до которого
 * клиент теперь знает всё: при `more` — последнего в странице, иначе — голова файла. */
export interface CommentListDto {
  seq: number;
  threads: ThreadView[];
  removed: string[];
  more: boolean;
}

export interface ThreadDetailDto {
  thread: ThreadView;
  comments: CommentDto[];
}

/** Ответ изменения. `removed` — обсуждение удалено (удалили первый комментарий); `thread` тогда — его последнее состояние. */
export interface CommentWriteDto {
  seq: number;
  thread: ThreadView;
  comment: CommentDto | null;
  removed: boolean;
}

export type CommentAction = 'create' | 'reply' | 'edit' | 'delete' | 'resolve' | 'reopen' | 'move';
const ACTIONS: readonly CommentAction[] = ['create', 'reply', 'edit', 'delete', 'resolve', 'reopen', 'move'];

/** Событие по WebSocket файла. `by` — кто изменил. `deleted` — id удалённого комментария (`action: 'delete'`). `gap` —
 * сервер пропустил события этому соединению (не успевал читать): клиент догоняет `?after=известный`. */
export type CommentEvent =
  | {
      kind: 'thread';
      seq: number;
      by: string;
      action: CommentAction;
      thread: ThreadDto;
      comment: CommentDto | null;
      deleted: string | null;
    }
  | { kind: 'removed'; seq: number; by: string; thread: string }
  | { kind: 'gap'; seq: number };

export interface MentionCandidate {
  id: string;
  name: string;
  /** Только участнику файла (прямая роль или команда): опубликованная ссылка адресов не раздаёт **(поведение)**. */
  email: string | null;
}

export interface MentionListDto {
  people: MentionCandidate[];
}

export interface PrefsDto {
  commentMail: boolean;
}

// --- Разбор ---

type Obj = Record<string, unknown>;

/** Объект ровно с этими полями: лишнее и недостающее — неверно. `optional` — поля, которых может не быть. */
function exact(v: unknown, keys: readonly string[], optional: readonly string[] = []): Obj | null {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return null;
  const o = v as Obj;
  for (const k of Object.keys(o)) if (!keys.includes(k) && !optional.includes(k)) return null;
  for (const k of keys) if (!Object.hasOwn(o, k)) return null;
  return o;
}

const isCoord = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v) && Math.abs(v) <= COORD_MAX;
const isCount = (v: unknown, max = Number.MAX_SAFE_INTEGER): v is number =>
  Number.isSafeInteger(v) && (v as number) >= 0 && (v as number) <= max;
const isTime = (v: unknown): v is number => isCount(v);
const isTreeId = (v: unknown): v is string => typeof v === 'string' && v.length <= 40 && isPageDoc(v);
const isName = (v: unknown): v is string => typeof v === 'string' && commentLength(v) <= MAX_NAME_CHARS;

export function parsePin(v: unknown): PinDto | null {
  const o = exact(v, ['page', 'node', 'x', 'y', 'wx', 'wy']);
  if (!o || !isTreeId(o.page) || !(o.node === null || isTreeId(o.node))) return null;
  if (!isCoord(o.x) || !isCoord(o.y) || !isCoord(o.wx) || !isCoord(o.wy)) return null;
  return { page: o.page, node: o.node, x: o.x, y: o.y, wx: o.wx, wy: o.wy };
}

/** Текст из запроса: нормальный и не больше `COMMENT_MENTIONS_MAX` разных упоминаний. */
function requestBody(v: unknown): string | null {
  const body = normalizeCommentBody(v);
  return body !== null && mentionIds(body).length <= COMMENT_MENTIONS_MAX ? body : null;
}

/** `POST …/comments`: новое обсуждение. */
export function parseNewThread(v: unknown): { pin: PinDto; body: string } | null {
  const o = exact(v, ['pin', 'body']);
  const pin = parsePin(o?.pin);
  const body = requestBody(o?.body);
  return pin && body !== null ? { pin, body } : null;
}

/** Ответ и правка: `{ body }`. */
export function parseCommentBody(v: unknown): { body: string } | null {
  const body = requestBody(exact(v, ['body'])?.body);
  return body !== null ? { body } : null;
}

/** `PATCH …/comments/:threadId`: ровно одно — закрыть/открыть или перенести метку. У них разные права, и одно событие
 * называет одно действие. */
export function parseThreadPatch(v: unknown): { resolved: boolean } | { pin: PinDto } | null {
  const o = exact(v, [], ['resolved', 'pin']);
  if (!o || Object.keys(o).length !== 1) return null;
  if ('resolved' in o) return typeof o.resolved === 'boolean' ? { resolved: o.resolved } : null;
  const pin = parsePin(o.pin);
  return pin ? { pin } : null;
}

export function parsePrefs(v: unknown): PrefsDto | null {
  const o = exact(v, ['commentMail']);
  return o && typeof o.commentMail === 'boolean' ? { commentMail: o.commentMail } : null;
}

function parseAuthor(v: unknown): CommentAuthor | null {
  const o = exact(v, ['id', 'name']);
  return o && (o.id === null || isCommentUserId(o.id)) && isName(o.name) ? { id: o.id, name: o.name } : null;
}

function parseMentionNames(v: unknown): Record<string, string> | null {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return null;
  const entries = Object.entries(v as Obj);
  if (entries.length > COMMENT_MENTIONS_MAX) return null;
  for (const [id, name] of entries) if (!USER_ID_RE.test(id) || !isName(name)) return null;
  // `fromEntries` заводит свои поля: id `__proto__` не подменит прототип.
  return Object.fromEntries(entries) as Record<string, string>;
}

function parseHead(v: unknown): CommentHeadDto | null {
  const o = exact(v, ['id', 'author', 'body', 'more', 'createdAt', 'editedAt', 'mentions']);
  if (!o || !isUuid(o.id) || !isTime(o.createdAt) || !(o.editedAt === null || isTime(o.editedAt))) return null;
  const author = parseAuthor(o.author);
  const mentions = parseMentionNames(o.mentions);
  if (!author || !mentions || !isPreview(o.body) || typeof o.more !== 'boolean') return null;
  return { id: o.id, author, body: o.body, more: o.more, createdAt: o.createdAt, editedAt: o.editedAt, mentions };
}

export function parseComment(v: unknown): CommentDto | null {
  const o = exact(v, ['id', 'thread', 'author', 'body', 'createdAt', 'editedAt', 'mentions']);
  if (!o || !isUuid(o.id) || !isUuid(o.thread) || !isTime(o.createdAt) || !(o.editedAt === null || isTime(o.editedAt))) return null;
  const author = parseAuthor(o.author);
  const mentions = parseMentionNames(o.mentions);
  // Текст с сервера уже нормальный: другой — подделка или сбой, показывать его нельзя.
  if (!author || !mentions || typeof o.body !== 'string' || normalizeCommentBody(o.body) !== o.body) return null;
  return { id: o.id, thread: o.thread, author, body: o.body, createdAt: o.createdAt, editedAt: o.editedAt, mentions };
}

const THREAD_KEYS = ['id', 'pin', 'author', 'createdAt', 'resolvedAt', 'resolvedBy', 'seq', 'first', 'replies', 'lastAt', 'participants'];

function threadFields(o: Obj): ThreadDto | null {
  if (!isUuid(o.id) || !isTime(o.createdAt) || !(o.resolvedAt === null || isTime(o.resolvedAt)) || !isCount(o.seq)) return null;
  if (!isCount(o.replies, THREAD_COMMENTS_MAX) || !isTime(o.lastAt)) return null;
  const pin = parsePin(o.pin);
  const author = parseAuthor(o.author);
  const resolvedBy = o.resolvedBy === null ? null : parseAuthor(o.resolvedBy);
  const first = parseHead(o.first);
  const p = o.participants;
  if (!pin || !author || (o.resolvedBy !== null && !resolvedBy) || !first) return null;
  if (!Array.isArray(p) || p.length > THREAD_PARTICIPANTS_MAX || !p.every(isCommentUserId)) return null;
  return {
    id: o.id,
    pin,
    author,
    createdAt: o.createdAt,
    resolvedAt: o.resolvedAt,
    resolvedBy,
    seq: o.seq,
    first,
    replies: o.replies,
    lastAt: o.lastAt,
    participants: [...p],
  };
}

export function parseThread(v: unknown): ThreadDto | null {
  const o = exact(v, THREAD_KEYS);
  return o ? threadFields(o) : null;
}

export function parseThreadView(v: unknown): ThreadView | null {
  const o = exact(v, [...THREAD_KEYS, 'unread']);
  const t = o && typeof o.unread === 'boolean' ? threadFields(o) : null;
  return t ? { ...t, unread: o!.unread as boolean } : null;
}

/** Событие с провода. Каждое поле проверено, лишние поля — неверно: старый клиент не примет полусмысленное. */
export function parseCommentEvent(v: unknown): CommentEvent | null {
  const kind = typeof v === 'object' && v !== null ? (v as Obj).kind : undefined;
  if (kind === 'gap') {
    const o = exact(v, ['kind', 'seq']);
    return o && isCount(o.seq) ? { kind: 'gap', seq: o.seq } : null;
  }
  if (kind === 'removed') {
    const o = exact(v, ['kind', 'seq', 'by', 'thread']);
    return o && isCount(o.seq) && isCommentUserId(o.by) && isUuid(o.thread)
      ? { kind: 'removed', seq: o.seq, by: o.by, thread: o.thread }
      : null;
  }
  if (kind !== 'thread') return null;
  const o = exact(v, ['kind', 'seq', 'by', 'action', 'thread', 'comment', 'deleted']);
  if (!o || !isCount(o.seq) || !isCommentUserId(o.by) || !ACTIONS.includes(o.action as CommentAction)) return null;
  const thread = parseThread(o.thread);
  const comment = o.comment === null ? null : parseComment(o.comment);
  if (!thread || thread.seq !== o.seq || (o.comment !== null && (!comment || comment.thread !== thread.id))) return null;
  if (!(o.deleted === null || isUuid(o.deleted))) return null;
  return { kind: 'thread', seq: o.seq, by: o.by, action: o.action as CommentAction, thread, comment, deleted: o.deleted };
}

const isArrayOf = <T>(v: unknown, of: (x: unknown) => T | null, max: number): T[] | null => {
  if (!Array.isArray(v) || v.length > max) return null;
  const out: T[] = [];
  for (const x of v) {
    const y = of(x);
    if (y === null) return null;
    out.push(y);
  }
  return out;
};

export function parseCommentList(v: unknown): CommentListDto | null {
  const o = exact(v, ['seq', 'threads', 'removed', 'more']);
  if (!o || !isCount(o.seq) || typeof o.more !== 'boolean') return null;
  const threads = isArrayOf(o.threads, parseThreadView, COMMENT_PAGE_MAX);
  const removed = isArrayOf(o.removed, (x) => (isUuid(x) ? x : null), COMMENT_PAGE_MAX);
  return threads && removed ? { seq: o.seq, threads, removed, more: o.more } : null;
}

export function parseThreadDetail(v: unknown): ThreadDetailDto | null {
  const o = exact(v, ['thread', 'comments']);
  const thread = parseThreadView(o?.thread);
  const comments = isArrayOf(o?.comments, parseComment, THREAD_COMMENTS_MAX);
  return thread && comments && comments.every((c) => c.thread === thread.id) ? { thread, comments } : null;
}

export function parseCommentWrite(v: unknown): CommentWriteDto | null {
  const o = exact(v, ['seq', 'thread', 'comment', 'removed']);
  if (!o || !isCount(o.seq) || typeof o.removed !== 'boolean') return null;
  const thread = parseThreadView(o.thread);
  const comment = o.comment === null ? null : parseComment(o.comment);
  return thread && (o.comment === null || comment) ? { seq: o.seq, thread, comment, removed: o.removed } : null;
}

export function parseMentionList(v: unknown): MentionListDto | null {
  const people = isArrayOf(
    exact(v, ['people'])?.people,
    (x) => {
      const o = exact(x, ['id', 'name', 'email']);
      return o && isCommentUserId(o.id) && isName(o.name) && (o.email === null || typeof o.email === 'string')
        ? { id: o.id, name: o.name, email: o.email }
        : null;
    },
    MENTION_SUGGEST_MAX,
  );
  return people ? { people } : null;
}

/** Строка поиска упоминаний: не длиннее `MENTION_QUERY_MAX`, без управляющих; пусто — все. */
export function parseMentionQuery(v: string | null): string | null {
  const q = (v ?? '').trim();
  // eslint-disable-next-line no-control-regex
  return commentLength(q) <= MENTION_QUERY_MAX && !/[\u0000-\u001f\u007f-\u009f]/.test(q) ? q : null;
}

// --- Кадр ---

const utf8 = new TextEncoder();
const fromUtf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** Тело кадра `comment`: JSON события. Больше `COMMENT_EVENT_MAX_BYTES` — ошибка отправителя, а не кадр на проводе. */
export function encodeCommentEvent(ev: CommentEvent): Uint8Array {
  const b = utf8.encode(JSON.stringify(ev));
  if (b.length > COMMENT_EVENT_MAX_BYTES) throw new RangeError(`событие комментария: ${b.length} байт`);
  return b;
}

export function decodeCommentEvent(b: Uint8Array): CommentEvent | null {
  if (b.length === 0 || b.length > COMMENT_EVENT_MAX_BYTES) return null;
  try {
    return parseCommentEvent(JSON.parse(fromUtf8.decode(b)));
  } catch {
    return null;
  }
}
