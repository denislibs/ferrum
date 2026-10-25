// Буфер обмена: байты Ferrum текстом за меткой и обратно (в том числе большие),
// выбор того, что вставлять, из события и из асинхронного API. Текст с меткой
// основной поток не декодирует: он уходит worker'у как есть (`kind: 'text'`).

import { expect, test } from '@playwright/test';
import {
  decodeText,
  encodeText,
  FERRUM_MIME,
  FERRUM_TEXT_MARKER,
  fromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
  MAX_CLIPBOARD_TEXT,
} from '../src/ui/clipboard';

test('байты → текст с меткой → байты', () => {
  const bytes = new Uint8Array([0x46, 0x45, 0x52, 0, 255, 128, 7]);
  const text = encodeText(bytes);
  expect(text.startsWith(FERRUM_TEXT_MARKER)).toBe(true);
  expect(decodeText(text)).toEqual(bytes);
  // 3 МБ — без переполнения стека.
  const big = new Uint8Array(3 * 1024 * 1024).map((_, i) => i % 251);
  expect(decodeText(encodeText(big))).toEqual(big);
});

test('чужой и испорченный текст — не наши байты', () => {
  expect(decodeText('привет')).toBeNull();
  expect(decodeText(`${FERRUM_TEXT_MARKER}***`)).toBeNull();
  expect(decodeText(`${FERRUM_TEXT_MARKER}${'A'.repeat(Math.ceil((MAX_CLIPBOARD_BYTES * 4) / 3) + 8)}`)).toBeNull();
});

function dt(text: string, files: File[] = []) {
  return { getData: (t: string) => (t === 'text/plain' ? text : ''), files: files as unknown as FileList };
}

test('вставка из события: свои байты важнее картинки, картинка — PNG/JPEG/WebP', () => {
  const png = new File([new Uint8Array([137, 80])], 'a.png', { type: 'image/png' });
  const gif = new File([new Uint8Array([71])], 'a.gif', { type: 'image/gif' });
  const marked = encodeText(new Uint8Array([1, 2]));
  expect(fromDataTransfer(dt(marked, [png]))).toEqual({ kind: 'text', text: marked });
  expect(fromDataTransfer(dt('текст', [gif, png]))).toMatchObject({ kind: 'image', type: 'image/png' });
  expect(fromDataTransfer(dt('текст', [gif]))).toBeNull();
  expect(fromDataTransfer(null)).toBeNull();
});

test('вставка из события: огромный текст с меткой отклоняется до декодирования', () => {
  const huge = FERRUM_TEXT_MARKER + 'A'.repeat(MAX_CLIPBOARD_TEXT);
  expect(fromDataTransfer(dt(huge))).toEqual({ kind: 'tooBig' });
});

test('вставка из асинхронного API: свой формат, текст с меткой, картинка', async () => {
  const item = (map: Record<string, Blob>) => ({ types: Object.keys(map), getType: async (t: string) => map[t] });
  expect(await fromClipboardItems([item({ [FERRUM_MIME]: new Blob([new Uint8Array([9, 8])]) })])).toEqual({
    kind: 'ferrum',
    bytes: new Uint8Array([9, 8]),
  });
  const marked = encodeText(new Uint8Array([5]));
  expect(await fromClipboardItems([item({ 'text/plain': new Blob([marked]) })])).toEqual({ kind: 'text', text: marked });
  expect(await fromClipboardItems([item({ 'text/plain': new Blob(['hello']), 'image/jpeg': new Blob([new Uint8Array([255])]) })])).toMatchObject({
    kind: 'image',
    type: 'image/jpeg',
  });
  expect(await fromClipboardItems([item({ 'text/html': new Blob(['<b>']) })])).toBeNull();
});

test('вставка из асинхронного API: размер проверяется до чтения', async () => {
  // Blob, который нельзя читать: размер виден, чтение — ошибка теста.
  const huge = (size: number) => ({ size, arrayBuffer: () => Promise.reject(new Error('читать нельзя')), text: () => Promise.reject(new Error('читать нельзя')) }) as unknown as Blob;
  const item = (type: string, blob: Blob) => ({ types: [type], getType: async () => blob });
  expect(await fromClipboardItems([item(FERRUM_MIME, huge(MAX_CLIPBOARD_BYTES + 1))])).toEqual({ kind: 'tooBig' });
  expect(await fromClipboardItems([item('text/plain', huge(MAX_CLIPBOARD_TEXT + 1))])).toEqual({ kind: 'tooBig' });
});
