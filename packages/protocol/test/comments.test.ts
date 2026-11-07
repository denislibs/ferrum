import { expect, test } from 'bun:test';
import {
  COMMENT_BODY_MAX,
  COMMENT_EVENT_MAX_BYTES,
  COMMENT_MENTIONS_MAX,
  commentLength,
  decodeCommentEvent,
  encodeCommentEvent,
  mentionIds,
  normalizeCommentBody,
  parseCommentBody,
  parseCommentEvent,
  parseCommentList,
  parseCommentWrite,
  parseMentionList,
  parseMentionQuery,
  parseNewThread,
  parsePrefs,
  parseThreadDetail,
  parseThreadPatch,
  splitCommentBody,
  THREAD_PARTICIPANTS_MAX,
  type CommentDto,
  type CommentEvent,
  type PinDto,
  type ThreadDto,
} from '../src/comments';
import { MAX_NAME_CHARS } from '../src/auth';

const T = '0b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d';
const C = '1b5c1d3e-7f40-4a8b-9c2d-1e2f3a4b5c6d';
const U = 'Xk2PqR8sT0uVwYz1aB3cD5eF7gH9iJ4l';
const pin: PinDto = { page: '3@7910247476534078275', node: '12@7910247476534078275', x: 1.5, y: -2, wx: 100, wy: 200 };
const comment = (over: Partial<CommentDto> = {}): CommentDto => ({
  id: C,
  thread: T,
  author: { id: U, name: 'Анна' },
  body: 'Привет, @[' + U + ']',
  createdAt: 1_700_000_000_000,
  editedAt: null,
  mentions: { [U]: 'Анна' },
  ...over,
});
const thread = (over: Partial<ThreadDto> = {}): ThreadDto => ({
  id: T,
  pin,
  author: { id: U, name: 'Анна' },
  createdAt: 1_700_000_000_000,
  resolvedAt: null,
  resolvedBy: null,
  seq: 7,
  first: comment(),
  replies: 0,
  lastAt: 1_700_000_000_000,
  participants: [U],
  ...over,
});

test('длина — в кодовых точках, а не в UTF-16', () => {
  expect(commentLength('😀😀')).toBe(2);
  expect('😀😀'.length).toBe(4);
  const emoji = '😀'.repeat(COMMENT_BODY_MAX);
  expect(normalizeCommentBody(emoji)).toBe(emoji);
  expect(normalizeCommentBody(emoji + 'а')).toBeNull();
  expect(normalizeCommentBody('а'.repeat(COMMENT_BODY_MAX))).not.toBeNull();
});

test('текст: только текст — переводы строк, управляющие, направление, края', () => {
  expect(normalizeCommentBody('  a\r\nb\rc\n\td  ')).toBe('a\nb\nc\n\td');
  expect(normalizeCommentBody('a\u0000b\u0007c\u001bd\u007fe\u0085f\u009fg')).toBe('abcdefg');
  // RLO переворачивает показ: «exe.txt» выглядел бы иначе, чем есть.
  expect(normalizeCommentBody('file\u202etxt.exe\u2066x\u2069')).toBe('filetxt.exex');
  // Метки направления (LRM, RLM, ALM) и встраивание LRE — тоже.
  expect(normalizeCommentBody('a\u200eb\u200fc\u061cd\u202ae')).toBe('abcde');
  // Одиночный суррогат в базу не идёт.
  expect(normalizeCommentBody('a\ud800b')).toBe('a�b');
  for (const bad of ['', '   ', '\u202e', '\r\n', 1, null, undefined, {}, ['a']]) expect(normalizeCommentBody(bad)).toBeNull();
  // HTML не трогается: это текст, показывают его как текст.
  expect(normalizeCommentBody('<b>x</b>')).toBe('<b>x</b>');
  // Нормальный текст не меняется при повторе: сервер и клиент считают одно.
  const once = normalizeCommentBody(' x\r\ny\u202e ')!;
  expect(normalizeCommentBody(once)).toBe(once);
});

test('упоминания: без повторов, по порядку, только вид `@[id]`', () => {
  expect(mentionIds('@[b] и @[a] и снова @[b], @[] @[a b] @[x/y] @[' + 'z'.repeat(65) + ']')).toEqual(['b', 'a']);
  expect(splitCommentBody('Эй, @[a] и @[b]!', { a: 'Анна' })).toEqual([
    { text: 'Эй, ' },
    { mention: 'a', name: 'Анна' },
    { text: ' и ' },
    { mention: 'b', name: null },
    { text: '!' },
  ]);
  // Имя из прототипа — не имя: `@[constructor]` не показывает функцию.
  expect(splitCommentBody('@[constructor]', {})).toEqual([{ mention: 'constructor', name: null }]);
});

test('запросы: ровно нужные поля, предел упоминаний', () => {
  expect(parseNewThread({ pin, body: ' x ' })).toEqual({ pin, body: 'x' });
  expect(parseNewThread({ pin, body: 'x', extra: 1 })).toBeNull();
  expect(parseNewThread({ pin: { ...pin, page: 'root' }, body: 'x' })).toBeNull();
  expect(parseNewThread({ pin: { ...pin, node: '007@1' }, body: 'x' })).toBeNull();
  for (const v of [NaN, Infinity, 1e9 + 1, '1']) expect(parseNewThread({ pin: { ...pin, x: v }, body: 'x' }), String(v)).toBeNull();
  expect(parseNewThread({ pin: { ...pin, node: null }, body: 'x' })?.pin.node).toBeNull();
  expect(parseNewThread({ pin, body: '' })).toBeNull();
  const many = (n: number) => Array.from({ length: n }, (_, i) => `@[u${i}]`).join(' ');
  expect(parseCommentBody({ body: many(COMMENT_MENTIONS_MAX) })).not.toBeNull();
  expect(parseCommentBody({ body: many(COMMENT_MENTIONS_MAX + 1) })).toBeNull();
  // Повторы одного человека — одно упоминание.
  expect(parseCommentBody({ body: '@[a] '.repeat(100) })).not.toBeNull();
  expect(parseCommentBody({ body: 'x', pin })).toBeNull();
  expect(parseThreadPatch({ resolved: true })).toEqual({ resolved: true });
  expect(parseThreadPatch({ pin })).toEqual({ pin });
  expect(parseThreadPatch({ resolved: true, pin })).toBeNull();
  expect(parseThreadPatch({})).toBeNull();
  expect(parseThreadPatch({ resolved: 1 })).toBeNull();
  expect(parsePrefs({ commentMail: false })).toEqual({ commentMail: false });
  expect(parsePrefs({ commentMail: 'no' })).toBeNull();
  expect(parseMentionQuery('  ан ')).toBe('ан');
  expect(parseMentionQuery(null)).toBe('');
  expect(parseMentionQuery('a'.repeat(65))).toBeNull();
  expect(parseMentionQuery('a\u0000')).toBeNull();
});

test('событие туда и обратно; каждое поле проверено', () => {
  const events: CommentEvent[] = [
    { kind: 'thread', seq: 7, by: U, action: 'create', thread: thread(), comment: comment(), deleted: null },
    {
      kind: 'thread',
      seq: 7,
      by: U,
      action: 'delete',
      thread: thread({ resolvedAt: 5, resolvedBy: { id: null, name: '' } }),
      comment: null,
      deleted: C,
    },
    { kind: 'removed', seq: 9, by: U, thread: T },
    { kind: 'gap', seq: 0 },
  ];
  for (const e of events) expect(decodeCommentEvent(encodeCommentEvent(e))).toEqual(e);
  const good = events[0] as Extract<CommentEvent, { kind: 'thread' }>;
  const bad: unknown[] = [
    { ...good, extra: 1 },
    { ...good, action: 'hack' },
    { ...good, seq: -1 },
    { ...good, seq: 8 },
    { ...good, by: '' },
    { ...good, deleted: 'x' },
    { ...good, thread: { ...thread(), unread: true } },
    { ...good, thread: thread({ participants: Array(THREAD_PARTICIPANTS_MAX + 1).fill(U) }) },
    { ...good, thread: thread({ first: comment({ thread: C }) }) },
    { ...good, thread: thread({ pin: { ...pin, wx: Infinity } }) },
    { ...good, comment: comment({ body: '<b>\u202e</b>' }) },
    { ...good, comment: comment({ body: ' x' }) },
    { ...good, comment: comment({ thread: C }) },
    { ...good, comment: comment({ author: { id: U } as never }) },
    { ...good, comment: comment({ mentions: { [U]: 1 } as never }) },
    { ...good, comment: comment({ editedAt: 1.5 }) },
    { kind: 'removed', seq: 1, by: U, thread: 'nope' },
    { kind: 'gap', seq: 1, extra: true },
    { kind: 'other', seq: 1 },
    null,
    [],
  ];
  for (const b of bad) expect(parseCommentEvent(b), JSON.stringify(b)).toBeNull();
  // Не JSON, не UTF-8, пусто, больше предела.
  expect(decodeCommentEvent(new TextEncoder().encode('{'))).toBeNull();
  expect(decodeCommentEvent(new Uint8Array([0xff]))).toBeNull();
  expect(decodeCommentEvent(new Uint8Array(0))).toBeNull();
  expect(decodeCommentEvent(new Uint8Array(COMMENT_EVENT_MAX_BYTES + 1))).toBeNull();
});

test('id `__proto__` в упоминаниях — своё поле, а не прототип', () => {
  const raw = `{"kind":"thread","seq":7,"by":"${U}","action":"create","thread":${JSON.stringify(thread())},"comment":${JSON.stringify(
    comment({ mentions: {} }),
  ).replace('"mentions":{}', '"mentions":{"__proto__":"x"}')},"deleted":null}`;
  const e = decodeCommentEvent(new TextEncoder().encode(raw));
  expect(e?.kind).toBe('thread');
  const m = (e as Extract<CommentEvent, { kind: 'thread' }>).comment!.mentions;
  expect(Object.getPrototypeOf(m)).toBe(Object.prototype);
  expect(Object.hasOwn(m, '__proto__')).toBe(true);
});

test('худшее событие помещается в кадр: два предельных текста, 20 упоминаний с предельными именами, 50 участников', () => {
  const name = '𝔄'.repeat(MAX_NAME_CHARS);
  const ids = Array.from({ length: COMMENT_MENTIONS_MAX }, (_, i) => `${'u'.repeat(60)}${String(i).padStart(4, '0')}`);
  const mentions = Object.fromEntries(ids.map((id) => [id, name]));
  const prefix = ids.map((id) => `@[${id}]`).join('');
  // Текст из четырёхбайтных знаков: дороже в JSON не бывает (экранированные знаки — два байта на кодовую точку).
  const body = prefix + '𝔄'.repeat(COMMENT_BODY_MAX - commentLength(prefix));
  expect(commentLength(body)).toBe(COMMENT_BODY_MAX);
  const author = { id: ids[0], name };
  const big = comment({ body, mentions, author });
  const ev: CommentEvent = {
    kind: 'thread',
    seq: Number.MAX_SAFE_INTEGER,
    by: ids[0],
    action: 'reply',
    thread: thread({
      seq: Number.MAX_SAFE_INTEGER,
      author,
      resolvedBy: author,
      resolvedAt: 1,
      first: big,
      participants: Array.from({ length: THREAD_PARTICIPANTS_MAX }, () => ids[0]),
    }),
    comment: { ...big, id: C },
    deleted: null,
  };
  const bytes = encodeCommentEvent(ev);
  console.log('худшее событие комментария, байт:', bytes.length);
  expect(bytes.length).toBeLessThanOrEqual(COMMENT_EVENT_MAX_BYTES);
  expect(decodeCommentEvent(bytes)).toEqual(ev);
});

test('ответы REST разбираются строго', () => {
  const view = { ...thread(), unread: true };
  expect(parseCommentList({ seq: 3, threads: [view], removed: [T], more: false })).toEqual({
    seq: 3,
    threads: [view],
    removed: [T],
    more: false,
  });
  expect(parseCommentList({ seq: 3, threads: [thread()], removed: [], more: false })).toBeNull();
  expect(parseThreadDetail({ thread: view, comments: [comment()] })?.comments).toHaveLength(1);
  expect(parseThreadDetail({ thread: view, comments: [comment({ thread: C })] })).toBeNull();
  expect(parseCommentWrite({ seq: 7, thread: view, comment: null, removed: true })?.removed).toBe(true);
  expect(parseMentionList({ people: [{ id: U, name: 'А', email: null }] })).toEqual({ people: [{ id: U, name: 'А', email: null }] });
  expect(parseMentionList({ people: Array(9).fill({ id: U, name: 'А', email: null }) })).toBeNull();
});
