// Маршруты в адресе после `#`: экран файлов, разделы, файл, новый файл.

import { expect, test } from '@playwright/test';
import { parseRoute, routeHash, sameRoute } from '../src/ui/router';

test('маршруты: разбор и обратно', () => {
  expect(parseRoute('')).toEqual({ kind: 'files', section: 'recent' });
  expect(parseRoute('#')).toEqual({ kind: 'files', section: 'recent' });
  expect(parseRoute('#/')).toEqual({ kind: 'files', section: 'recent' });
  expect(parseRoute('#/drafts')).toEqual({ kind: 'files', section: 'drafts' });
  expect(parseRoute('#/trash/')).toEqual({ kind: 'files', section: 'trash' });
  expect(parseRoute('#/new')).toEqual({ kind: 'new' });
  expect(parseRoute('#/file/0f8e-12_ab')).toEqual({ kind: 'file', id: '0f8e-12_ab' });
  // Неизвестный путь — экран файлов.
  expect(parseRoute('#/что-то/ещё')).toEqual({ kind: 'files', section: 'recent' });
  for (const r of [
    { kind: 'files', section: 'recent' },
    { kind: 'files', section: 'drafts' },
    { kind: 'files', section: 'trash' },
    { kind: 'new' },
    { kind: 'file', id: 'abc-123' },
  ] as const) {
    expect(parseRoute(routeHash(r))).toEqual(r);
  }
  expect(sameRoute(parseRoute(''), parseRoute('#/'))).toBe(true);
});

test('идентификатор файла из адреса не доверенный', () => {
  // Неверный id — маршрут к файлу без id: интерфейс покажет «Файл не найден».
  expect(parseRoute('#/file/' + 'a'.repeat(65))).toEqual({ kind: 'file', id: '' });
  expect(parseRoute('#/file/%E0%A4%A')).toEqual({ kind: 'file', id: '' });
  expect(parseRoute('#/file/a%2Fb')).toEqual({ kind: 'file', id: '' });
  expect(parseRoute('#/file/<script>')).toEqual({ kind: 'file', id: '' });
  expect(parseRoute('#/file/a'.padEnd(72, 'a'))).toEqual({ kind: 'file', id: '' });
});
