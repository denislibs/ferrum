// Секция «Текст»: разбор интерлиньяжа и трекинга, группы выбора шрифта и поиск,
// начертание при смене семейства, окно виртуального списка.

import { expect, test } from '@playwright/test';
import { MAX_FONT_SIZE, MAX_LETTER_SPACING_PERCENT, MAX_LINE_HEIGHT_PERCENT, MIN_FONT_SIZE } from '../src/core/limits';
import { clampSpacing, fieldValue, fontRows, formatSpacing, parseSpacing, isImeKey, rowWindow, stepRow, styleFor } from '../src/ui/text-panel';

test('интерлиньяж и трекинг: авто, проценты, пиксели', () => {
  expect(parseSpacing('Авто', true)).toEqual({ unit: 'auto', value: 0 });
  expect(parseSpacing('', true)).toEqual({ unit: 'auto', value: 0 });
  expect(parseSpacing('', false)).toBeNull();
  expect(parseSpacing('150%', true)).toEqual({ unit: 'percent', value: 150 });
  expect(parseSpacing('-0,5', false)).toEqual({ unit: 'px', value: -0.5 });
  expect(parseSpacing('20px', true)).toEqual({ unit: 'px', value: 20 });
  expect(parseSpacing('abc', true)).toBeNull();
  expect(formatSpacing({ unit: 'percent', value: 120 })).toBe('120%');
  expect(formatSpacing({ unit: 'auto', value: 0 })).toBe('Авто');
});

const catalog = [
  { family: 'Roboto', source: 'bundled' as const, styles: ['Regular', 'Bold'] },
  { family: 'Inter', source: 'bundled' as const, styles: ['Regular', 'Bold', 'Italic'] },
  { family: 'Arial', source: 'system' as const, styles: ['Regular'] },
];

test('группы шрифтов и поиск', () => {
  const rows = fontRows(catalog, ['Comic Sans', 'Inter'], '');
  expect(rows.map((r) => (r.kind === 'heading' ? `#${r.label}` : r.family))).toEqual([
    '#В документе',
    'Comic Sans',
    'Inter',
    '#Вшитые',
    'Inter',
    'Roboto',
    '#Системные',
    'Arial',
  ]);
  const found = fontRows(catalog, [], 'rob');
  expect(found).toEqual([{ kind: 'heading', label: 'Вшитые' }, { kind: 'font', family: 'Roboto', source: 'bundled' }]);
  expect(fontRows(catalog, [], 'zzz')).toEqual([]);
});

test('начертание при смене семейства и окно списка', () => {
  expect(styleFor(catalog, 'Roboto', 'Bold')).toBe('Bold');
  expect(styleFor(catalog, 'Roboto', 'Italic')).toBe('Regular');
  expect(styleFor(catalog, 'Нет такого', 'Bold')).toBe('Regular');
  expect(rowWindow(0, 280, 28, 100)).toEqual({ start: 0, end: 22 });
  expect(rowWindow(28 * 50, 280, 28, 100)).toEqual({ start: 44, end: 66 });
  expect(rowWindow(28 * 95, 280, 28, 100).end).toBe(100);
});

// Ядро отклоняет значения вне пределов ошибкой (не зажимает): секция зажимает сама.
test('интерлиньяж и трекинг зажимаются в пределы ядра', () => {
  expect(parseSpacing('-5', true)).toEqual({ unit: 'px', value: 0 });
  expect(parseSpacing('-5%', true)).toEqual({ unit: 'percent', value: 0 });
  expect(parseSpacing('99999', true)).toEqual({ unit: 'px', value: MAX_FONT_SIZE });
  expect(parseSpacing('2000%', true)).toEqual({ unit: 'percent', value: 2000 });
  expect(parseSpacing('20000%', true)).toEqual({ unit: 'percent', value: MAX_LINE_HEIGHT_PERCENT });
  expect(parseSpacing('99999', false)).toEqual({ unit: 'px', value: MAX_FONT_SIZE });
  expect(parseSpacing('-99999', false)).toEqual({ unit: 'px', value: -MAX_FONT_SIZE });
  expect(parseSpacing('-5', false)).toEqual({ unit: 'px', value: -5 });
  expect(parseSpacing('2000%', false)).toEqual({ unit: 'percent', value: MAX_LETTER_SPACING_PERCENT });
  expect(parseSpacing('-2000%', false)).toEqual({ unit: 'percent', value: -MAX_LETTER_SPACING_PERCENT });
  expect(clampSpacing({ unit: 'auto', value: 0 }, true)).toEqual({ unit: 'auto', value: 0 });
  expect(clampSpacing({ unit: 'px', value: -1 }, true)).toEqual({ unit: 'px', value: 0 });
  expect(clampSpacing({ unit: 'percent', value: 1e9 }, false)).toEqual({ unit: 'percent', value: MAX_LETTER_SPACING_PERCENT });
});

test('значение числового поля зажимается, в том числе прибавка к «Смешанным»', () => {
  const size = [MIN_FONT_SIZE, MAX_FONT_SIZE] as const;
  expect(fieldValue({ kind: 'set', value: 40 }, 24, ...size)).toBe(40);
  expect(fieldValue({ kind: 'set', value: 0 }, 24, ...size)).toBe(1);
  expect(fieldValue({ kind: 'set', value: 20000 }, 24, ...size)).toBe(10000);
  // Перетаскивание подписи у «Смешанных»: показано 12, сдвиг влево на 50 — не меньше 1.
  expect(fieldValue({ kind: 'add', delta: -50 }, 12, ...size)).toBe(1);
  expect(fieldValue({ kind: 'add', delta: 50000 }, 12, ...size)).toBe(10000);
  expect(fieldValue({ kind: 'add', delta: 5 }, 12, ...size)).toBe(17);
  expect(fieldValue({ kind: 'add', delta: -50 }, 12, 0, MAX_FONT_SIZE)).toBe(0);
  expect(fieldValue({ kind: 'add', delta: -50000 }, 0, -MAX_FONT_SIZE, MAX_FONT_SIZE)).toBe(-10000);
});

test('Enter подтверждения IME — не команда; стрелки шагают по семействам, минуя заголовки', () => {
  expect(isImeKey({ isComposing: true, keyCode: 13 })).toBe(true);
  expect(isImeKey({ isComposing: false, keyCode: 229 })).toBe(true);
  expect(isImeKey({ isComposing: false, keyCode: 13 })).toBe(false);
  const rows = fontRows(catalog, ['Comic Sans'], ''); // #В документе, Comic, #Вшитые, Inter, Roboto, #Системные, Arial
  expect(stepRow(rows, -1, 1)).toBe(1);
  expect(stepRow(rows, 1, 1)).toBe(3);
  expect(stepRow(rows, 4, 1)).toBe(6);
  expect(stepRow(rows, 6, 1)).toBe(6);
  expect(stepRow(rows, 3, -1)).toBe(1);
  expect(stepRow(rows, 1, -1)).toBe(1);
});
