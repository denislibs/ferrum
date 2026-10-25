// Размер картинки по заголовку — до декодирования (PNG, JPEG, WebP трёх видов).

import { expect, test } from '@playwright/test';
import { imageSize } from '../src/core/image-size';

const bytes = (...parts: (number[] | string)[]) =>
  new Uint8Array(parts.flatMap((p) => (typeof p === 'string' ? [...p].map((c) => c.charCodeAt(0)) : p)));

test('PNG: IHDR', () => {
  const png = bytes([0x89], 'PNG', [13, 10, 26, 10, 0, 0, 0, 13], 'IHDR', [0, 0, 0x01, 0x00, 0, 0, 0, 0x80]);
  expect(imageSize(png)).toEqual({ width: 256, height: 128, type: 'png' });
});

test('JPEG: маркер SOF после APP0, огромный размер читается без декодирования', () => {
  const app0 = [0xff, 0xe0, 0, 16, ...new Array(14).fill(0)];
  const sof = [0xff, 0xc2, 0, 17, 8, 0xea, 0x60, 0xea, 0x60, 3, ...new Array(9).fill(0)];
  expect(imageSize(bytes([0xff, 0xd8], app0, sof))).toEqual({ width: 60000, height: 60000, type: 'jpeg' });
  expect(imageSize(bytes([0xff, 0xd8], app0))).toBeNull();
});

test('WebP: VP8, VP8L, VP8X', () => {
  const head = (kind: string) => bytes('RIFF', [0, 0, 0, 0], 'WEBP', kind, [0, 0, 0, 0]);
  const vp8 = new Uint8Array([...head('VP8 '), 0, 0, 0, 0x9d, 0x01, 0x2a, 0x40, 0x01, 0xf0, 0x00, 0, 0]);
  expect(imageSize(vp8)).toEqual({ width: 320, height: 240, type: 'webp' });
  // VP8L: 14 бит ширины и высоты (минус один) после подписи 0x2f.
  const w = 99;
  const h = 49;
  const bits = w | (h << 14);
  const vp8l = new Uint8Array([...head('VP8L'), 0x2f, bits & 0xff, (bits >> 8) & 0xff, (bits >> 16) & 0xff, (bits >> 24) & 0xff, 0, 0, 0, 0]);
  expect(imageSize(vp8l)).toEqual({ width: 100, height: 50, type: 'webp' });
  const vp8x = new Uint8Array([...head('VP8X'), 0, 0, 0, 0, 0x3f, 0x1f, 0, 0x0f, 0x27, 0]);
  expect(imageSize(vp8x)).toEqual({ width: 8000, height: 10000, type: 'webp' });
});

test('не картинка — null', () => {
  expect(imageSize(bytes('GIF89a', [1, 0, 1, 0]))).toBeNull();
  expect(imageSize(new Uint8Array([]))).toBeNull();
});
