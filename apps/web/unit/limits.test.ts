// Пределы интерфейса (`src/core/limits.ts`) совпадают с константами ядра на Rust и с
// числом в `public/theme-init.js` (он не импортирует модули) — как `THEME_CANVAS`.

import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import {
  MAX_DATA_BYTES,
  MAX_FAMILY_NAME,
  MAX_FONT_BYTES,
  MAX_FONT_FACES,
  MAX_FONT_FAMILIES,
  MAX_IMAGE_SIDE,
  MAX_JOURNAL_ENTRIES,
  MAX_KEYMAP_BYTES,
  MAX_NAME_CHARS,
  MAX_SETTINGS_BYTES,
  tooLarge,
} from '../src/core/limits';

const read = (rel: string) => readFileSync(new URL(rel, import.meta.url), 'utf8');

/** Значение `pub const NAME: тип = выражение;` в файле Rust: числа, `*` и `<<`. */
function rustConst(file: string, name: string): number {
  const m = new RegExp(`pub const ${name}: \\w+ = ([^;]+);`).exec(read(file));
  if (!m) throw new Error(`нет ${name} в ${file}`);
  const expr = m[1].replace(/_/g, '').trim();
  const shift = /^(\d+)\s*<<\s*(\d+)$/.exec(expr);
  if (shift) return Number(shift[1]) * 2 ** Number(shift[2]);
  if (!/^\d+(\s*\*\s*\d+)*$/.test(expr)) throw new Error(`не разобрать ${name} = ${expr}`);
  return expr.split('*').reduce((a, b) => a * Number(b), 1);
}

const CRATES = '../../../crates/';

test('пределы совпадают с константами ядра', () => {
  expect(rustConst(`${CRATES}ferrum-wasm/src/core/panels.rs`, 'MAX_PASTE_BYTES')).toBe(MAX_DATA_BYTES);
  expect(rustConst(`${CRATES}ferrum-editor/src/clipboard.rs`, 'MAX_JSON')).toBe(MAX_DATA_BYTES);
  expect(rustConst(`${CRATES}ferrum-editor/src/clipboard.rs`, 'MAX_IMAGE_BYTES')).toBe(MAX_DATA_BYTES);
  expect(rustConst(`${CRATES}ferrum-render/src/image.rs`, 'MAX_IMAGE_SIDE')).toBe(MAX_IMAGE_SIDE);
  expect(rustConst(`${CRATES}ferrum-editor/src/keymap.rs`, 'MAX_KEYMAP_JSON')).toBe(MAX_KEYMAP_BYTES);
  expect(rustConst(`${CRATES}ferrum-editor/src/pages.rs`, 'MAX_NAME_CHARS')).toBe(MAX_NAME_CHARS);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_JOURNAL_BATCHES')).toBe(MAX_JOURNAL_ENTRIES);
  // Шрифты (подсистема 3): файл, имя семейства, размер списка.
  expect(rustConst(`${CRATES}ferrum-fontmeta/src/lib.rs`, 'MAX_FONT_FILE')).toBe(MAX_FONT_BYTES);
  expect(rustConst(`${CRATES}ferrum-text/src/fonts.rs`, 'MAX_FAMILY_NAME')).toBe(MAX_FAMILY_NAME);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_FONT_FAMILIES')).toBe(MAX_FONT_FAMILIES);
  expect(rustConst(`${CRATES}ferrum-wasm/src/protocol.rs`, 'MAX_FONT_FACES')).toBe(MAX_FONT_FACES);
  // Десктоп отдаёт список системных шрифтов уже в этих пределах.
  const desktop = '../../desktop/src-tauri/src/fonts.rs';
  expect(rustConst(desktop, 'MAX_FONT_FAMILIES')).toBe(MAX_FONT_FAMILIES);
  expect(rustConst(desktop, 'MAX_FONT_FACES')).toBe(MAX_FONT_FACES);
  expect(rustConst(desktop, 'MAX_FAMILY_NAME')).toBe(MAX_FAMILY_NAME);
});

test('theme-init.js читает настройки с тем же пределом', () => {
  const m = /raw\.length <= (\d+)/.exec(read('../public/theme-init.js'));
  expect(m, 'предел записи в theme-init.js').not.toBeNull();
  expect(Number(m![1])).toBe(MAX_SETTINGS_BYTES);
});

test('тексты о размере строятся из пределов', () => {
  expect(tooLarge(MAX_DATA_BYTES)).toBe('Слишком большой объём данных: предел — 64 МБ');
  expect(tooLarge(MAX_DATA_BYTES, MAX_DATA_BYTES + 1)).toBe('Слишком большой объём данных: 65 МБ, предел — 64 МБ');
});
