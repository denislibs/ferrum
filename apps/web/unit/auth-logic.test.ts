import { expect, test } from '@playwright/test';
import { AUTH_ERROR_CODES, confirmHash, parseConfirmHash, RESEND_SECONDS } from '@ferrum/protocol';
import { avatarHue, failureText, initials, resendLeft, screenFor } from '../src/ui/auth/auth-logic';

const USER = { id: 'u1', email: 'ivan@example.com', name: 'Иван' };
const TOKEN = 'a'.repeat(32);

test('без сессии — вход, запрошенный адрес запоминается; служебные адреса — нет', () => {
  expect(screenFor('#/file/abc', null)).toEqual({ kind: 'login', next: '#/file/abc' });
  expect(screenFor('', null)).toEqual({ kind: 'login', next: null });
  expect(screenFor('#/', null)).toEqual({ kind: 'login', next: '#/' });
});

test('с сессией: приложение, а без имени — сначала экран имени', () => {
  expect(screenFor('#/file/abc', USER)).toEqual({ kind: 'app', user: USER });
  expect(screenFor('#/file/abc', { ...USER, name: '' })).toEqual({ kind: 'name', user: { ...USER, name: '' } });
});

test('страница подтверждения видна и с сессией, и без неё', () => {
  const hash = confirmHash(TOKEN, '#/file/abc');
  expect(screenFor(hash, null)).toEqual({ kind: 'confirm', token: TOKEN, next: '#/file/abc' });
  expect(screenFor(hash, USER)).toEqual({ kind: 'confirm', token: TOKEN, next: '#/file/abc' });
  expect(screenFor('#/auth/confirm?token=short', null)).toEqual({ kind: 'confirm', token: null, next: null });
});

test('повторная отправка — через 60 с, секунды округляются вверх', () => {
  expect(RESEND_SECONDS).toBe(60);
  expect(resendLeft(1000, 1000)).toBe(60);
  expect(resendLeft(1000, 1001)).toBe(60);
  expect(resendLeft(1000, 59_500)).toBe(2);
  expect(resendLeft(1000, 61_000)).toBe(0);
  expect(resendLeft(1000, 99_000)).toBe(0);
});

test('у каждой ошибки — понятный текст', () => {
  expect(failureText('INVALID_TOKEN')).toContain('недействительна или устарела');
  expect(failureText('TOO_MANY_REQUESTS')).toContain('Слишком много');
  expect(failureText('NETWORK')).toContain('Сервер недоступен');
  for (const f of ['BAD_REQUEST', 'BAD_ORIGIN', 'UNAUTHORIZED', 'NOT_FOUND'] as const) expect(failureText(f).length).toBeGreaterThan(5);
});

test('инициалы и оттенок кружка', () => {
  expect(initials({ name: 'Иван Петров', email: 'x@y.ru' })).toBe('ИП');
  expect(initials({ name: 'иван', email: 'x@y.ru' })).toBe('И');
  expect(initials({ name: 'Анна Мария Шмидт', email: 'x@y.ru' })).toBe('АМ');
  expect(initials({ name: '', email: 'ivan@y.ru' })).toBe('I');
  expect(avatarHue('u1')).toBe(avatarHue('u1'));
  expect(avatarHue('u1')).not.toBe(avatarHue('u2'));
  expect(avatarHue('длинный-идентификатор')).toBeLessThan(360);
});

test('служебные адреса кроме страницы подтверждения: ничем не отличаются от остальных', () => {
  const user = { id: 'u1', email: 'x@y.ru', name: 'Иван' };
  for (const hash of ['#/auth/', '#/auth/foo', '#/auth/confirmX', '#/auth']) {
    expect(parseConfirmHash(hash)).toBeNull();
    expect(screenFor(hash, null)).toEqual({ kind: 'login', next: hash === '#/auth' ? '#/auth' : null });
    expect(screenFor(hash, user)).toEqual({ kind: 'app', user });
  }
  expect(screenFor('#/auth/confirm', null)).toEqual({ kind: 'confirm', token: null, next: null });
});

test('сбой сервера показывается как сбой сервера', () => {
  expect(failureText('UNAVAILABLE')).toContain('Сервер недоступен');
  for (const code of AUTH_ERROR_CODES) expect(failureText(code).length).toBeGreaterThan(0);
});
