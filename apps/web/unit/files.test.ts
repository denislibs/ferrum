// Имя скачиваемого файла, подписи экрана файлов («Изменён …», размер), разделы,
// поиск и окна списка, проверка запросов к worker'у хранилища.

import { expect, test } from '@playwright/test';
import { cleanName, nameFromFile, requestError, sectionList, windowOf, MAX_WINDOW, type FileMeta } from '../src/core/file-list';
import { fileName, formatSize, relativeTime } from '../src/ui/files';
import { filesLayout, filesWindow } from '../src/ui/files-layout';

test('имя файла очищается', () => {
  expect(fileName('Макет', '.ferrum')).toBe('Макет.ferrum');
  expect(fileName('', '.ferrum')).toBe('Без названия.ferrum');
  expect(fileName('  ', '.ferrum')).toBe('Без названия.ferrum');
  expect(fileName('a/b\\c:d*e?f"g<h>i|j', '.ferrum')).toBe('a_b_c_d_e_f_g_h_i_j.ferrum');
  expect(fileName('..', '.ferrum')).toBe('Без названия.ferrum');
  expect(fileName(' .скрытый. ', '.ferrum')).toBe('скрытый.ferrum');
  expect(fileName('a\u0000\u001fb', '.ferrum')).toBe('a__b.ferrum');
  expect(fileName('я'.repeat(500), '.ferrum')).toBe(`${'я'.repeat(120)}.ferrum`);
});

test('относительное время: «только что» до минуты, дальше — минуты, часы, дни', () => {
  const now = Date.UTC(2026, 9, 25, 12, 0, 0);
  const min = 60_000;
  expect(relativeTime(now, now)).toBe('только что');
  expect(relativeTime(now - 59_999, now)).toBe('только что');
  // Часы другой вкладки спешат — не «через минуту».
  expect(relativeTime(now + 5 * min, now)).toBe('только что');
  expect(relativeTime(NaN, now)).toBe('только что');
  expect(relativeTime(now - min, now)).toBe('1 минуту назад');
  expect(relativeTime(now - 5 * min, now)).toBe('5 минут назад');
  expect(relativeTime(now - 59 * min, now)).toBe('59 минут назад');
  expect(relativeTime(now - 60 * min, now)).toBe('1 час назад');
  expect(relativeTime(now - 3 * 60 * min, now)).toBe('3 часа назад');
  expect(relativeTime(now - 24 * 60 * min, now)).toBe('вчера');
  expect(relativeTime(now - 2 * 24 * 60 * min, now)).toBe('позавчера');
  expect(relativeTime(now - 10 * 24 * 60 * min, now)).toBe('10 дней назад');
  expect(relativeTime(now - 60 * 24 * 60 * min, now)).toBe('2 месяца назад');
  expect(relativeTime(now - 800 * 24 * 60 * min, now)).toBe('2 года назад');
});

test('размер файла', () => {
  expect(formatSize(0)).toBe('—');
  expect(formatSize(512)).toBe('512 Б');
  expect(formatSize(12 * 1024)).toBe('12 КБ');
  expect(formatSize(1.2 * 1024 * 1024)).toBe('1,2 МБ');
  expect(formatSize(3 * 1024 ** 3)).toBe('3 ГБ');
});

const meta = (id: string, name: string, t: Partial<FileMeta> = {}): FileMeta => ({
  id,
  name,
  createdAt: 0,
  editedAt: 0,
  openedAt: 0,
  size: 0,
  thumbVersion: 0,
  version: 0,
  ...t,
});

test('разделы: недавние — по открытию, черновики — по изменению, корзина — отдельно', () => {
  const all = [
    meta('a', 'Альфа', { openedAt: 30, editedAt: 10 }),
    meta('b', 'бета', { openedAt: 10, editedAt: 30 }),
    meta('c', 'Гамма', { openedAt: 20, editedAt: 20, trashedAt: 5 }),
    meta('d', 'Дельта', { openedAt: 0, editedAt: 40 }),
    // Равное время — по имени, затем по id: порядок не прыгает.
    meta('f', 'Ёж', { openedAt: 30, editedAt: 10 }),
    meta('e', 'Ёж', { openedAt: 30, editedAt: 10 }),
  ];
  const ids = (l: FileMeta[]) => l.map((m) => m.id);
  expect(ids(sectionList(all, 'recent', ''))).toEqual(['a', 'e', 'f', 'b']);
  expect(ids(sectionList(all, 'drafts', ''))).toEqual(['d', 'b', 'a', 'e', 'f']);
  expect(ids(sectionList(all, 'trash', ''))).toEqual(['c']);
  // Поиск — по имени без учёта регистра, пробелы по краям не считаются.
  expect(ids(sectionList(all, 'drafts', ' БЕТ '))).toEqual(['b']);
  expect(ids(sectionList(all, 'drafts', 'ёж'))).toEqual(['e', 'f']);
  expect(ids(sectionList(all, 'drafts', 'нет такого'))).toEqual([]);
  expect(ids(sectionList(all, 'trash', 'гам'))).toEqual(['c']);
});

test('окно списка — не больше 200 строк, неверные границы безопасны', () => {
  const list = Array.from({ length: 1000 }, (_, i) => i);
  expect(windowOf(list, 10, 5)).toEqual([10, 11, 12, 13, 14]);
  expect(windowOf(list, 0, 10_000)).toHaveLength(MAX_WINDOW);
  expect(windowOf(list, -5, 2)).toEqual([0, 1]);
  expect(windowOf(list, 999, 10)).toEqual([999]);
  expect(windowOf(list, 2000, 10)).toEqual([]);
});

test('имя записи: из имени на диске, без управляющих символов, с пределом', () => {
  expect(nameFromFile('Макет.ferrum')).toBe('Макет');
  expect(nameFromFile('Макет.FERRUM')).toBe('Макет');
  expect(nameFromFile('.ferrum')).toBe('Без названия');
  expect(cleanName('  a\u0000b\n ')).toBe('a b');
  expect([...cleanName('я'.repeat(300))]).toHaveLength(256);
});

test('запросы к хранилищу не доверенные: неверное — ошибка, а не сбой', () => {
  expect(requestError({ id: 1, op: 'list', section: 'recent', query: '', start: 0, count: 200 })).toBeNull();
  expect(requestError({ id: 1, op: 'list', section: 'recent', query: '', start: 0, count: 201 })).not.toBeNull();
  expect(requestError({ id: 1, op: 'list', section: 'всё', query: '', start: 0, count: 10 })).not.toBeNull();
  expect(requestError({ id: 1, op: 'list', section: 'recent', query: 'x'.repeat(5000), start: 0, count: 10 })).not.toBeNull();
  expect(requestError({ id: 1, op: 'list', section: 'recent', query: '', start: NaN, count: 10 })).not.toBeNull();
  expect(requestError({ id: 1, op: 'get', file: 'x'.repeat(65) })).not.toBeNull();
  expect(requestError({ id: 1, op: 'get', file: '' })).not.toBeNull();
  expect(requestError({ id: 1, op: 'get', file: 'abc' })).toBeNull();
  const w = { id: 1, op: 'write', file: 'a', bytes: new ArrayBuffer(4), thumb: null, base: 0, journal: null };
  expect(requestError(w)).toBeNull();
  expect(requestError({ ...w, thumb: 'keep' })).toBeNull();
  expect(requestError({ ...w, thumb: 'другое' })).not.toBeNull();
  expect(requestError({ ...w, base: undefined })).not.toBeNull();
  expect(requestError({ ...w, base: -1 })).not.toBeNull();
  expect(requestError({ ...w, bytes: new Uint8Array(4) })).not.toBeNull();
  expect(requestError({ ...w, thumb: new ArrayBuffer(9 << 20) })).toMatch(/Слишком большой объём данных/);
  // Строки — до 4096 байт UTF-8: кириллица — два байта на знак.
  const ren = { id: 1, op: 'rename', file: 'a', name: '' };
  expect(requestError({ ...ren, name: 'я'.repeat(2048) })).toBeNull();
  expect(requestError({ ...ren, name: 'я'.repeat(2049) })).not.toBeNull();
  expect(requestError({ id: 1, op: 'blob', file: 'a' })).toBeNull();
  expect(requestError({ id: 1, op: 'create', name: 'n', bytes: new Blob(['x']) })).toBeNull();
  expect(requestError({ id: 1, op: 'create', name: 'n', bytes: 'текст' })).not.toBeNull();
  expect(requestError({ id: 1, op: 'seed', count: 100_000 })).not.toBeNull();
  expect(requestError({ id: 1.5, op: 'emptyTrash' })).not.toBeNull();
  expect(requestError({ id: 1, op: 'нет такого' })).not.toBeNull();
  expect(requestError(null)).not.toBeNull();
});

test('раскладка экрана файлов: столбцы по ширине, окно по прокрутке', () => {
  const g = filesLayout(1000, 'grid');
  expect(g.cols).toBe(4);
  expect(g.cardW * 4 + 16 * 3).toBeCloseTo(1000);
  expect(g.thumbH).toBe(Math.round((g.cardW * 2) / 3));
  // Узко — одна колонка не уже наименьшей карточки.
  expect(filesLayout(100, 'grid')).toMatchObject({ cols: 1, cardW: 200 });
  const l = filesLayout(800, 'list');
  expect(l).toMatchObject({ cols: 1, rowH: 48 });
  // 1000 записей, видно ~15 строк: окно — видимые с запасом, не весь список.
  expect(filesWindow(0, 720, l)).toEqual({ start: 0, count: 16 });
  expect(filesWindow(48 * 100, 720, l)).toEqual({ start: 99, count: 17 });
  expect(filesWindow(0, 100_000, l).count).toBe(MAX_WINDOW);
  expect(filesWindow(g.rowH * 3, 600, g).start).toBe(2 * g.cols);
});

test('журнал: запросы к хранилищу проверяются', () => {
  const buf = new ArrayBuffer(4);
  const ok = { id: 1, op: 'appendJournal', file: 'f1', session: 1_700_000_000_000_123, batch: 3, bytes: buf };
  expect(requestError(ok)).toBeNull();
  for (const bad of [
    { ...ok, session: -1 },
    { ...ok, session: 1.5 },
    { ...ok, batch: Number.MAX_SAFE_INTEGER + 2 },
    { ...ok, bytes: 'x' },
    { ...ok, bytes: new Uint8Array(4) },
    { ...ok, file: '' },
  ]) {
    expect(requestError(bad), JSON.stringify(bad)).toBe('Неверный запрос к хранилищу файлов');
  }
  const write = { id: 2, op: 'write', file: 'f1', bytes: buf, thumb: 'keep', base: 1, journal: null };
  expect(requestError(write)).toBeNull();
  expect(requestError({ ...write, journal: { session: 5, upTo: 2 } })).toBeNull();
  for (const journal of [{ session: 5 }, { session: 'x', upTo: 1 }, { session: 1, upTo: -3 }, 7, undefined]) {
    expect(requestError({ ...write, journal }), JSON.stringify(journal)).toBe('Неверный запрос к хранилищу файлов');
  }
  expect(requestError({ id: 3, op: 'journalCount', file: 'f1' })).toBeNull();
  expect(requestError({ id: 3, op: 'journalCount', file: 7 })).toBe('Неверный запрос к хранилищу файлов');
});
