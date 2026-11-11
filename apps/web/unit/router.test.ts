// Маршруты в адресе после `#`: экран файлов, разделы, файл, новый файл.
import { extensions } from '../src/ui/extensions';

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

test('ссылка на страницу: `?page=` разбирается, неверная страница отбрасывается', () => {
  expect(parseRoute('#/file/abc?page=12@345')).toEqual({ kind: 'file', id: 'abc', page: '12@345' });
  expect(parseRoute(routeHash({ kind: 'file', id: 'abc', page: '1@2' }))).toEqual({ kind: 'file', id: 'abc', page: '1@2' });
  expect(routeHash({ kind: 'file', id: 'abc', page: '1@2' })).toBe('#/file/abc?page=1%402');
  for (const bad of ['<script>', '1@', '@2', '1@2@3', '1'.repeat(70) + '@2', '']) {
    expect(parseRoute(`#/file/abc?page=${encodeURIComponent(bad)}`)).toEqual({ kind: 'file', id: 'abc' });
  }
  expect(parseRoute('#/file/abc?other=1')).toEqual({ kind: 'file', id: 'abc' });
});

test('чужие разделы — только если их знает сборка', () => {
  expect(parseRoute('#/shared')).toEqual({ kind: 'files', section: 'recent' });
  expect(parseRoute('#/project/abc')).toEqual({ kind: 'files', section: 'recent' });
  extensions.parseSection = (path) => (path === 'shared' ? 'shared' : path === 'project/abc' ? 'project:abc' : null);
  try {
    expect(parseRoute('#/shared')).toEqual({ kind: 'files', section: 'shared' });
    expect(parseRoute('#/project/abc')).toEqual({ kind: 'files', section: 'project:abc' });
    expect(parseRoute('#/other')).toEqual({ kind: 'files', section: 'recent' });
    expect(routeHash({ kind: 'files', section: 'shared' })).toBe('#/shared');
    expect(routeHash({ kind: 'files', section: 'project:abc' })).toBe('#/project/abc');
  } finally {
    delete extensions.parseSection;
  }
});

test('новый файл в проекте и токен ссылки — из адреса; мусор отбрасывается', () => {
  expect(parseRoute('#/new?project=abc')).toEqual({ kind: 'new', project: 'abc' });
  expect(parseRoute('#/new?project=../x')).toEqual({ kind: 'new' });
  expect(routeHash({ kind: 'new', project: 'abc' })).toBe('#/new?project=abc');
  expect(parseRoute('#/file/abc?link=Tok-en_1')).toEqual({ kind: 'file', id: 'abc', link: 'Tok-en_1' });
  expect(parseRoute('#/file/abc?link=a%2Fb')).toEqual({ kind: 'file', id: 'abc' });
  // Токен в запись истории не попадает: адрес файла — без него.
  expect(routeHash({ kind: 'file', id: 'abc', link: 'x' })).toBe('#/file/abc');
});

test('ссылка на обсуждение: `?comment=` — только UUID', () => {
  const id = '0f8fad5b-d9cb-469f-a165-70867728950e';
  expect(parseRoute(`#/file/abc?comment=${id}`)).toEqual({ kind: 'file', id: 'abc', comment: id });
  expect(parseRoute('#/file/abc?comment=../x')).toEqual({ kind: 'file', id: 'abc' });
  expect(parseRoute('#/file/abc?comment=0F8FAD5B-D9CB-469F-A165-70867728950E')).toEqual({ kind: 'file', id: 'abc' });
});
