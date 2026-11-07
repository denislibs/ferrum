// Панель и поле комментариев: фильтры и порядок, окно виртуального списка, упоминания и длина текста (та же, что у сервера).

import { expect, test } from '@playwright/test';
import { COMMENT_BODY_MAX, normalizeCommentBody, type ThreadView } from '@ferrum/protocol';
import {
  authorName,
  bodyState,
  DEFAULT_FILTER,
  editableBody,
  insertMention,
  listWindow,
  mentionAt,
  mentionName,
  panelThreads,
  popoverPlace,
  storedBody,
} from '../src/ui/server/comments-logic';

const t = (id: string, o: Partial<ThreadView> & { page?: string }): ThreadView => {
  const author = o.author ?? { id: 'a', name: 'Анна' };
  return {
    id,
    pin: { page: o.page ?? '1@1', node: null, x: 0, y: 0, wx: 0, wy: 0 },
    author,
    createdAt: 0,
    resolvedAt: null,
    resolvedBy: null,
    seq: 1,
    first: { id: `${id}c`, thread: id, author, body: 'текст', createdAt: 0, editedAt: null, mentions: {} },
    replies: 0,
    lastAt: 0,
    participants: [author.id ?? ''],
    unread: false,
    ...o,
  };
};

test('панель: страница, закрытые скрыты по умолчанию, «только мои», новые сверху и непрочитанные первыми', () => {
  const list = [
    t('1', { lastAt: 10 }),
    t('2', { lastAt: 30, page: '2@1' }),
    t('3', { lastAt: 20, resolvedAt: 5 }),
    t('4', { lastAt: 5, unread: true, participants: ['a', 'me'] }),
  ];
  expect(panelThreads(list, DEFAULT_FILTER, '1@1', 'me').map((x) => x.id)).toEqual(['1', '4']);
  expect(panelThreads(list, { ...DEFAULT_FILTER, scope: 'all', resolved: true }, '1@1', 'me').map((x) => x.id)).toEqual([
    '2',
    '3',
    '1',
    '4',
  ]);
  expect(panelThreads(list, { ...DEFAULT_FILTER, sort: 'unread' }, '1@1', 'me').map((x) => x.id)).toEqual(['4', '1']);
  expect(panelThreads(list, { ...DEFAULT_FILTER, mine: true }, '1@1', 'me').map((x) => x.id)).toEqual(['4']);
});

test('окно списка: 1000 обсуждений — рисуются только видимые строки с запасом', () => {
  expect(listWindow(0, 600, 76, 1000)).toEqual({ start: 0, end: 12 });
  expect(listWindow(76 * 500, 600, 76, 1000)).toEqual({ start: 496, end: 512 });
  expect(listWindow(76 * 2000, 600, 76, 1000)).toEqual({ start: 1000, end: 1000 });
});

test('упоминание: набор после @, вставка имени, на сервер — @[id]; правка возвращает имена', () => {
  expect(mentionAt('привет @ан', 10)).toEqual({ start: 7, query: 'ан' });
  expect(mentionAt('mail@example', 12)).toBeNull();
  expect(mentionAt('@', 1)).toEqual({ start: 0, query: '' });
  const ins = insertMention('привет @ан', 7, 10, 'Анна Мария');
  expect(ins).toEqual({ text: 'привет @Анна Мария ', caret: 19 });
  const picked = [
    { id: 'u1', name: 'Анна' },
    { id: 'u2', name: 'Анна Мария' },
  ];
  expect(storedBody('@Анна Мария и @Анна, а @Борис нет', picked)).toBe('@[u2] и @[u1], а @Борис нет');
  expect(editableBody('@[u2] и @[zz]', { u2: 'Анна Мария' })).toEqual({
    text: '@Анна Мария и @[zz]',
    picked: [{ id: 'u2', name: 'Анна Мария' }],
  });
});

test('длина — в кодовых точках, как у сервера: 4000 эмодзи можно, 4001 — нет', () => {
  const max = '😀'.repeat(COMMENT_BODY_MAX);
  expect(bodyState(max)).toEqual({ ok: true, length: COMMENT_BODY_MAX, over: false });
  expect(normalizeCommentBody(max)).toBe(max);
  expect(bodyState(max + '😀').ok).toBe(false);
  expect(normalizeCommentBody(max + '😀')).toBeNull();
  expect(bodyState('   ').ok).toBe(false);
});

test('подпись автора: удалённый и без имени', () => {
  expect(authorName({ id: null, name: 'x' })).toBe('Удалённый пользователь');
  expect(authorName({ id: 'a', name: ' ' })).toBe('Участник');
  expect(authorName({ id: 'a', name: '<b>Ив</b>' })).toBe('<b>Ив</b>');
});

test('упоминание без имени — «Участник» и в тексте, и в правке; не из списка — «неизвестный» и `@[id]` в правке', () => {
  expect(mentionName('')).toBe('Участник');
  expect(mentionName('  ')).toBe('Участник');
  expect(mentionName(null)).toBe('неизвестный');
  expect(mentionName('Анна')).toBe('Анна');
  const e = editableBody('для @[u3] и @[zz]', { u3: '' });
  expect(e).toEqual({ text: 'для @Участник и @[zz]', picked: [{ id: 'u3', name: 'Участник' }] });
  // Сохранение без правки — то же упоминание.
  expect(storedBody(e.text, e.picked)).toBe('для @[u3] и @[zz]');
});

test('окно у метки: справа сверху от неё и целиком в пределах холста', () => {
  const box = { left: 100, top: 40, width: 800, height: 600 };
  const size = { width: 320, height: 200 };
  expect(popoverPlace({ x: 200, y: 200 }, box, size)).toEqual({ left: 336, top: 212 });
  // У правого нижнего края — внутрь холста с отступом.
  expect(popoverPlace({ x: 790, y: 590 }, box, size)).toEqual({ left: 100 + 800 - 320 - 8, top: 40 + 600 - 200 - 8 });
  // У левого верхнего — не левее и не выше отступа.
  expect(popoverPlace({ x: -50, y: 0 }, box, size)).toEqual({ left: 108, top: 48 });
  // Холст меньше окна — прижато к левому верхнему краю (длинное прокручивается внутри).
  expect(popoverPlace({ x: 100, y: 100 }, { left: 0, top: 0, width: 200, height: 150 }, size)).toEqual({ left: 8, top: 8 });
});
