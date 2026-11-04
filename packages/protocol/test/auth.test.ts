import { expect, test } from 'bun:test';
import {
  confirmHash,
  MAX_NAME_CHARS,
  parseConfirmHash,
  parseConfirmRequest,
  parseEmail,
  parseNameRequest,
  parseNext,
  parseSignInRequest,
} from '../src/index';

const TOKEN = 'a'.repeat(32);

test('адрес приводится к нижнему регистру, мусор отвергается', () => {
  expect(parseEmail('  Ivan@Example.COM ')).toBe('ivan@example.com');
  for (const bad of ['', 'ivan', 'ivan@', '@x.ru', 'a b@x.ru', 'a@x', 5, null, `${'a'.repeat(250)}@x.ru`, 'a\n@x.ru']) {
    expect(parseEmail(bad)).toBeNull();
  }
});

test('возврат после входа: только маршрут приложения', () => {
  expect(parseNext('#/file/abc?page=1@2')).toBe('#/file/abc?page=1@2');
  for (const bad of [
    'https://evil.example',
    '//evil.example',
    '/file/x',
    '#file',
    '#/auth/confirm?token=x',
    `#/${'a'.repeat(600)}`,
    7,
    '#/a\nb',
  ]) {
    expect(parseNext(bad)).toBeNull();
  }
});

test('запрос ссылки: лишние поля не проходят дальше, неверный возврат — отказ', () => {
  expect(parseSignInRequest({ email: 'A@x.ru', callbackURL: 'https://evil.example', name: 'x' })).toEqual({ email: 'a@x.ru' });
  expect(parseSignInRequest({ email: 'a@x.ru', next: '#/file/1' })).toEqual({ email: 'a@x.ru', next: '#/file/1' });
  expect(parseSignInRequest({ email: 'a@x.ru', next: 'https://evil.example' })).toBeNull();
  expect(parseSignInRequest([])).toBeNull();
  expect(parseSignInRequest(null)).toBeNull();
});

test('подтверждение: токен из 32 букв', () => {
  expect(parseConfirmRequest({ token: TOKEN })).toEqual({ token: TOKEN });
  for (const bad of [{ token: 'a' }, { token: `${TOKEN}!` }, { token: 5 }, {}, 'x']) expect(parseConfirmRequest(bad)).toBeNull();
});

test('имя: пробелы убираются, пустое и длинное не годятся', () => {
  expect(parseNameRequest({ name: '  Иван  ' })).toEqual({ name: 'Иван' });
  expect(parseNameRequest({ name: 'я'.repeat(MAX_NAME_CHARS) })).not.toBeNull();
  for (const bad of [{ name: '   ' }, { name: 'я'.repeat(MAX_NAME_CHARS + 1) }, { name: 'a\u0007' }, { name: 1 }, {}]) {
    expect(parseNameRequest(bad)).toBeNull();
  }
});

test('адрес подтверждения: туда и обратно, чужое — не он', () => {
  const hash = confirmHash(TOKEN, '#/file/abc?page=1@2');
  expect(parseConfirmHash(hash)).toEqual({ token: TOKEN, next: '#/file/abc?page=1@2' });
  expect(parseConfirmHash(confirmHash(TOKEN))).toEqual({ token: TOKEN, next: null });
  expect(parseConfirmHash('#/auth/confirm?token=short')).toEqual({ token: null, next: null });
  expect(parseConfirmHash('#/file/1')).toBeNull();
  expect(parseConfirmHash('')).toBeNull();
});
