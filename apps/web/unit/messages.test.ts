// Пределы байтов в командах: основной поток отказывает до отправки (и до копии в
// память wasm), worker проверяет ещё раз. Текст вставки — свой предел.

import { expect, test } from '@playwright/test';
import { clipboardBytesError, commandSizeError, MAX_COMMAND_BYTES, pasteTextError } from '../src/core/messages';
import { FERRUM_TEXT_MARKER, MAX_CLIPBOARD_TEXT } from '../src/core/clipboard-text';

test('pasteImage: больше 64 МБ — отказ на русском', () => {
  expect(MAX_COMMAND_BYTES.pasteImage).toBe(64 * 1024 * 1024);
  const big = new Uint8Array(MAX_COMMAND_BYTES.pasteImage + 1);
  expect(commandSizeError({ type: 'pasteImage', bytes: big })).toBe(
    'Слишком большой объём данных: 65 МБ, предел для этой команды — 64 МБ.',
  );
  expect(commandSizeError({ type: 'pasteImage', bytes: new Uint8Array(8) })).toBeNull();
  expect(commandSizeError({ type: 'pasteImage', bytes: [1, 2] as unknown as Uint8Array })).toBe(
    'Неверная команда: байты должны быть Uint8Array.',
  );
});

test('текст вставки: строка, не длиннее предела', () => {
  expect(pasteTextError(`${FERRUM_TEXT_MARKER}AQI=`)).toBeNull();
  expect(pasteTextError(42 as unknown as string)).toBe('Неверная команда: текст вставки должен быть строкой.');
  expect(pasteTextError('A'.repeat(MAX_CLIPBOARD_TEXT + 1))).toBe(
    'Слишком большой объём данных в буфере обмена: предел — 64 МБ.',
  );
});

test('байты для текста буфера: Uint8Array не больше 64 МБ', () => {
  expect(clipboardBytesError(new Uint8Array(8))).toBeNull();
  expect(clipboardBytesError(new Uint8Array(64 * 1024 * 1024 + 1))).toBe('Не удалось скопировать: выделение 65 МБ, предел — 64 МБ');
  expect(clipboardBytesError('x' as unknown as Uint8Array)).toBe('Неверная команда: байты должны быть Uint8Array.');
});
