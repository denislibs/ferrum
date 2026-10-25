// Поле числа: выражения, проценты, «Смешанные», шаг перетаскивания подписи с
// Shift ×10 и Alt ×0.1, Enter и Esc, формат.

import { expect, test } from '@playwright/test';
import { arrowStep, fieldKey, formatNumber, parseFieldInput, scrubValue } from '../src/ui/number';

const set = (value: number) => ({ kind: 'set', value });

test('выражения', () => {
  const p = (t: string, current = 100) => parseFieldInput(t, { current });
  expect(p('120+16')).toEqual(set(136));
  expect(p('2*(3+4)')).toEqual(set(14));
  expect(p('10 - 2 * 3')).toEqual(set(4));
  expect(p('-5')).toEqual(set(-5));
  // Запятая — тоже десятичный разделитель.
  expect(p('12,5')).toEqual(set(12.5));
  // Начатое с оператора продолжает текущее значение.
  expect(p('*2', 40)).toEqual(set(80));
  expect(p('/4', 40)).toEqual(set(10));
  expect(p('+16', 40)).toEqual(set(56));
  expect(p('1e3')).toEqual(set(1000));
});

test('проценты: от текущего или от заданной базы', () => {
  expect(parseFieldInput('50%', { current: 300 })).toEqual(set(150));
  expect(parseFieldInput('50%', { current: 80, percentOf: 100 })).toEqual(set(50));
  expect(parseFieldInput('100%+10', { current: 40 })).toEqual(set(50));
});

test('у «Смешанных» +N и -N — прибавка к каждому', () => {
  expect(parseFieldInput('+10', { current: 5, mixed: true })).toEqual({ kind: 'add', delta: 10 });
  expect(parseFieldInput('-2.5', { current: 5, mixed: true })).toEqual({ kind: 'add', delta: -2.5 });
  expect(parseFieldInput('20', { current: 5, mixed: true })).toEqual(set(20));
});

test('не число — null', () => {
  for (const bad of ['', '  ', 'abc', '1+', '(1', '1)', '2**3', '1/0', 'Смешанные']) {
    expect(parseFieldInput(bad, { current: 1 }), bad).toBeNull();
  }
});

test('перетаскивание подписи: шаг, Shift ×10, Alt ×0.1', () => {
  expect(scrubValue(100, 7, 1, { shift: false, alt: false })).toBe(107);
  expect(scrubValue(100, 7, 1, { shift: true, alt: false })).toBe(170);
  expect(scrubValue(100, 7, 1, { shift: false, alt: true })).toBe(100.7);
  expect(scrubValue(0.1, 2, 0.1, { shift: false, alt: false })).toBe(0.3);
  expect(scrubValue(10, -3.4, 1, { shift: false, alt: false })).toBe(7);
});

test('клавиши поля: Enter применяет, Esc откатывает, стрелки — шаг', () => {
  expect(fieldKey('Enter')).toBe('commit');
  expect(fieldKey('NumpadEnter')).toBe('commit');
  expect(fieldKey('Escape')).toBe('revert');
  expect(fieldKey('a')).toBeNull();
  expect(arrowStep('ArrowUp', false)).toBe(1);
  expect(arrowStep('ArrowDown', true)).toBe(-10);
  expect(arrowStep('Tab', false)).toBe(0);
});

test('формат числа', () => {
  expect(formatNumber(120)).toBe('120');
  expect(formatNumber(12.3456)).toBe('12.35');
  expect(formatNumber(-0.0001)).toBe('0');
  expect(formatNumber(1440, 0)).toBe('1440');
  expect(formatNumber(NaN)).toBe('');
});
