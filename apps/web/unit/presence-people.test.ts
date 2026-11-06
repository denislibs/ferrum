// Полоса аватаров (спека подсистемы 4, раздел 9): люди из участников, инициалы, постоянная подпись кнопки.

import { expect, test } from '@playwright/test';
import { avatarLabel, groupPeople, initials } from '../src/ui/server/presence-people';

const page = '3@1';

test('инициалы — по видимым символам: эмодзи и суррогатные пары не ломаются', () => {
  expect(initials('Анна Смирнова')).toBe('АС');
  expect(initials('  борис  ')).toBe('Б');
  expect(initials('')).toBe('?');
  expect(initials('😀 Смайл')).toBe('😀С');
  expect(initials('𝒜лиса')).toBe('𝒜');
  // Флаг — две кодовые точки, семья — несколько, соединённых ZWJ: один видимый символ.
  expect(initials('🇷🇺 Россия')).toBe('🇷🇺Р');
  expect(initials('👨‍👩‍👧 Семья')).toBe('👨‍👩‍👧С');
  // Буква с отдельным диакритическим знаком.
  expect(initials('émile')).toBe('É');
});

test('люди: по человеку один, себя нет, следовать — за новейшим соединением; на другой странице — если все вкладки там', () => {
  const people = groupPeople(
    [
      { id: 1, user: 'me', name: 'Я', color: '#000000', page },
      { id: 2, user: 'a', name: 'Анна', color: '#0d74ce', page },
      { id: 5, user: 'a', name: 'Анна', color: '#0d74ce', page: '4@1' },
      { id: 3, user: 'b', name: 'Борис', color: '#12a594', page: '4@1' },
      { id: 4, user: 'c', name: 'Вера', color: '#ab6400', page: null },
    ],
    'me',
    page,
  );
  expect(people).toEqual([
    { user: 'a', name: 'Анна', color: '#0d74ce', participant: 5, ids: [2, 5], elsewhere: false },
    { user: 'b', name: 'Борис', color: '#12a594', participant: 3, ids: [3], elsewhere: true },
    { user: 'c', name: 'Вера', color: '#ab6400', participant: 4, ids: [4], elsewhere: false },
  ]);
});

test('подпись аватара не зависит от следования: имя и где человек', () => {
  expect(avatarLabel({ name: 'Анна', elsewhere: false })).toBe('Анна');
  expect(avatarLabel({ name: 'Анна', elsewhere: true })).toBe('Анна — на другой странице');
});
