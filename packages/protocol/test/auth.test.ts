import { expect, test } from 'bun:test';
import {
  AUTH_ERROR_CODES,
  confirmHash,
  mailbox,
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

test('варианты строки с тем же ящиком адресом не считаются, настоящие адреса проходят', () => {
  for (const bad of [
    'x<box@d.ru>',
    'x <box@d.ru>',
    'z,box@d.ru',
    'box@d.ru,z@d.ru',
    '(c)box@d.ru',
    'box@(c)d.ru',
    'q:box@d.ru;',
    '"q"@d.ru',
    'a;b@d.ru',
    'a[b@d.ru',
    'a\\b@d.ru',
    'a@b@d.ru',
    'r%box@d.ru',
    'r!box@d.ru',
    '.box@d.ru',
    'box.@d.ru',
    'b..ox@d.ru',
    'box@d..ru',
    'box@-d.ru',
    'box@d-.ru',
    'box@d.ru.',
    'box\u200b@d.ru',
    'box@d_e.ru',
  ]) {
    expect(parseEmail(bad)).toBeNull();
  }
  for (const good of ['box@d.ru', 'first.last+tag@sub.d-e.ru', "o'brien@d.ru", 'a_b-c@d.ru', 'иван@почта.рф', '1@2.ru']) {
    expect(parseEmail(good)).toBe(good);
  }
  expect(parseEmail('ｂｏｘ@d.ru')).toBe('box@d.ru');
});

test('ящик для счётчиков: метка и точки не дают новый счётчик', () => {
  expect(mailbox('box@d.ru')).toBe('box@d.ru');
  expect(mailbox('b.o.x+tag@d.ru')).toBe('box@d.ru');
  expect(mailbox('box+a+b@sub.d.ru')).toBe('box@sub.d.ru');
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

test('коды ошибок: список один, дублей нет', () => {
  expect(new Set(AUTH_ERROR_CODES).size).toBe(AUTH_ERROR_CODES.length);
  expect(AUTH_ERROR_CODES).toContain('UNAVAILABLE');
});
