// Нечёткий поиск «Быстрых действий»: подпоследовательность с весом за начало
// слова, буквы подряд, начало и всё имя; регистр, «ё» и раскладка не важны.
// Те же правила — у поиска слоёв в ядре (`crates/ferrum-editor/tests/find_layers.rs`).

import { expect, test } from '@playwright/test';
import { fuzzyScore, layoutVariants, MAX_TEXT_CHARS, queryVariants } from '../src/ui/fuzzy';

test('буквы запроса — по порядку, не обязательно подряд', () => {
  expect(fuzzyScore('Кнопка отправить', 'кно')).not.toBeNull();
  expect(fuzzyScore('Кнопка отправить', 'котп')).not.toBeNull();
  expect(fuzzyScore('Кнопка', 'кнопки')).toBeNull();
  expect(fuzzyScore('Кнопка', 'акн')).toBeNull();
  expect(fuzzyScore('Кнопка', '')).toBeNull();
  expect(fuzzyScore('Кнопка', '   ')).toBeNull();
});

test('веса: начало слова, буквы подряд, имя целиком', () => {
  const s = (t: string, q: string) => fuzzyScore(t, q) ?? -Infinity;
  expect(s('Order summary', 'su')).toBeGreaterThan(s('Ordersum', 'su'));
  expect(s('Карточка товара', 'кар')).toBeGreaterThan(s('Кнопка в ряд', 'кар'));
  expect(s('Фон', 'фон')).toBeGreaterThan(s('Фон карточки', 'фон'));
  expect(s('Слой 5', 'слой 5')).toBeGreaterThan(s('Слой 59', 'слой 5'));
  // Лучшее из выравниваний: «вп» — начала слов «Вставить поверх», а не «в» из «Вставить».
  expect(s('Вставить поверх', 'вп')).toBeGreaterThan(s('Вставить', 'вп'));
});

test('регистр, «ё» и раскладка не важны; пробелы запроса не учитываются', () => {
  expect(fuzzyScore('ЁЛКА', 'елка')).toBe(fuzzyScore('ёлка', 'ЕЛКА'));
  expect(fuzzyScore('Order', 'щквук')).toBe(fuzzyScore('Order', 'order'));
  expect(fuzzyScore('Файл', 'afqk')).not.toBeNull();
  expect(fuzzyScore('Шапка сайта', 'шап сай')).not.toBeNull();
  expect(layoutVariants('щквук')).toContain('order');
  expect(queryVariants('ab c').map((v) => v.join(''))).toContain('abc');
  expect(queryVariants('  ')).toEqual([]);
});

test('длинный текст участвует первыми 40 символами', () => {
  expect(MAX_TEXT_CHARS).toBe(40);
  expect(fuzzyScore('а'.repeat(40) + 'я', 'я')).toBeNull();
  expect(fuzzyScore('а'.repeat(39) + 'я', 'я')).not.toBeNull();
});

// Те же значения — в `crates/ferrum-editor/tests/find_layers.rs`: счёт один.
test('веса совпадают с ядром', () => {
  const vectors: [string, string, number][] = [
    ['Кнопка отправить', 'кно', 27],
    ['Кнопка отправить', 'котп', 33],
    ['Order summary', 'su', 12],
    ['Ordersum', 'su', 4],
    ['Фон', 'фон', 37],
    ['Слой 5', 'слой 5', 51],
    ['Слой 59', 'слой 5', 41],
    ['Вставить поверх', 'вп', 21],
    ['Шапка сайта', 'шап сай', 45],
    ['Order', 'щквук', 49],
  ];
  for (const [text, query, score] of vectors) expect(fuzzyScore(text, query), `${text} / ${query}`).toBe(score);
});
