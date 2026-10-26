// Пределы байтов в командах: основной поток отказывает до отправки (и до копии в
// память wasm), worker проверяет ещё раз. Текст вставки — свой предел.

import { expect, test } from '@playwright/test';
import { clipboardBytesError, commandSizeError, copyTooLarge, MAX_COMMAND_BYTES, pasteTextError } from '../src/core/messages';
import { FERRUM_TEXT_MARKER, MAX_CLIPBOARD_TEXT } from '../src/core/clipboard-text';

test('pasteImage: больше 64 МБ — отказ на русском', () => {
  expect(MAX_COMMAND_BYTES.pasteImage).toBe(64 * 1024 * 1024);
  const big = new Uint8Array(MAX_COMMAND_BYTES.pasteImage + 1);
  expect(commandSizeError({ type: 'pasteImage', bytes: big })).toBe(
    'Слишком большой объём данных: 65 МБ, предел — 64 МБ',
  );
  expect(commandSizeError({ type: 'pasteImage', bytes: new Uint8Array(8) })).toBeNull();
  expect(commandSizeError({ type: 'pasteImage', bytes: [1, 2] as unknown as Uint8Array })).toBe(
    'Неверная команда: данные не в виде байтов',
  );
});

test('текст вставки: строка, не длиннее предела', () => {
  expect(pasteTextError(`${FERRUM_TEXT_MARKER}AQI=`)).toBeNull();
  expect(pasteTextError(42 as unknown as string)).toBe('Неверная команда: вставка не текстом');
  expect(pasteTextError('A'.repeat(MAX_CLIPBOARD_TEXT + 1))).toBe('Слишком большой объём данных: предел — 64 МБ');
});

test('байты для текста буфера: Uint8Array не больше 64 МБ', () => {
  expect(clipboardBytesError(new Uint8Array(8))).toBeNull();
  expect(clipboardBytesError(new Uint8Array(64 * 1024 * 1024 + 1))).toBe('Не удалось скопировать: выделение 65 МБ, предел — 64 МБ');
  expect(clipboardBytesError('x' as unknown as Uint8Array)).toBe('Неверная команда: данные не в виде байтов');
});

test('тексты отказов — по голосу дизайн-системы: без точки в конце, без имён типов', () => {
  const shared = new Uint8Array(new SharedArrayBuffer(8));
  const texts = [
    commandSizeError({ type: 'open', bytes: new Uint8Array(MAX_COMMAND_BYTES.open + 1) }),
    commandSizeError({ type: 'paste', bytes: 'x' as unknown as Uint8Array }),
    commandSizeError({ type: 'paste', bytes: shared }),
    pasteTextError(1 as unknown as string),
    pasteTextError('A'.repeat(MAX_CLIPBOARD_TEXT + 1)),
    clipboardBytesError('x' as unknown as Uint8Array),
    clipboardBytesError(shared),
    clipboardBytesError(new Uint8Array(64 * 1024 * 1024 + 1)),
    copyTooLarge(70 * 1024 * 1024, true),
  ];
  for (const t of texts) {
    expect(t).toBeTruthy();
    expect(t!, t!).not.toMatch(/\.$/);
    expect(t!, t!).not.toMatch(/Uint8Array|SharedArrayBuffer|undefined|null/);
  }
  // «Слишком большой объём» — один текст на все пределы.
  const big = texts.filter((t) => t!.includes('объём'));
  expect(big.length).toBeGreaterThanOrEqual(2);
  for (const t of big) expect(t).toMatch(/^Слишком большой объём данных: (\d+ МБ, )?предел — \d+ МБ$/);
});

test('openWithJournal: файл и журнал вместе — не больше 512 МБ, пачки — байтами, буферы передаются', async () => {
  const { prepareCommand } = await import('../src/core/messages');
  const { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } = await import('../src/core/limits');
  const file = new Uint8Array(16);
  const ok = { type: 'openWithJournal' as const, bytes: file, journal: [new Uint8Array(8), new Uint8Array(4)] };
  expect(commandSizeError(ok)).toBeNull();
  const half = new Uint8Array(MAX_OPEN_BYTES / 2 + 1);
  expect(commandSizeError({ type: 'openWithJournal', bytes: half, journal: [half] })).toBe(
    'Слишком большой объём данных: 513 МБ, предел — 512 МБ',
  );
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: ['x' as unknown as Uint8Array] })).toBe(
    'Неверная команда: данные не в виде байтов',
  );
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: 'x' as unknown as Uint8Array[] })).toBe(
    'Неверная команда: данные не в виде байтов',
  );
  const many = new Array<Uint8Array>(MAX_JOURNAL_ENTRIES + 1).fill(new Uint8Array(1));
  expect(commandSizeError({ type: 'openWithJournal', bytes: file, journal: many })).toBe(
    'Неверная команда: в журнале изменений больше 100000 записей',
  );
  // Буферы передаются: файл, каждая пачка один раз (общий буфер — одной записью),
  // часть чужого буфера — копией.
  const shared = new Uint8Array(32);
  const part = new Uint8Array(new ArrayBuffer(64), 8, 8);
  const p = prepareCommand({ type: 'openWithJournal', bytes: file, journal: [shared, shared, part] });
  expect('error' in p).toBe(false);
  if ('error' in p) return;
  expect(p.transfer).toHaveLength(3);
  const cmd = p.cmd as { journal: Uint8Array[] };
  expect(cmd.journal[2].byteLength).toBe(8);
  expect(cmd.journal[2].buffer.byteLength).toBe(8);
});
