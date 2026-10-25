// Размер картинки по заголовку — до декодирования. Вставленный из буфера JPEG или
// WebP в несколько килобайт может объявить 60 000 × 60 000 пикселей: декодировать
// его, чтобы узнать размер, значит отдать worker'у гигабайты памяти. Поэтому
// размер читается из заголовка (PNG — IHDR, JPEG — маркер SOF, WebP — VP8/VP8L/VP8X).

export interface ImageSize {
  width: number;
  height: number;
  type: 'png' | 'jpeg' | 'webp';
}

const be16 = (b: Uint8Array, i: number) => (b[i] << 8) | b[i + 1];
const le16 = (b: Uint8Array, i: number) => b[i] | (b[i + 1] << 8);
const le24 = (b: Uint8Array, i: number) => b[i] | (b[i + 1] << 8) | (b[i + 2] << 16);
const be32 = (b: Uint8Array, i: number) => ((b[i] << 24) >>> 0) + ((b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3]);
const ascii = (b: Uint8Array, i: number, s: string) => [...s].every((c, k) => b[i + k] === c.charCodeAt(0));

/** Маркеры JPEG с размером кадра (SOF0–SOF15 без DHT, JPG и DAC). */
const SOF = new Set([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf]);

function jpeg(b: Uint8Array): ImageSize | null {
  let i = 2;
  while (i + 9 < b.length) {
    if (b[i] !== 0xff) return null;
    const m = b[i + 1];
    if (m === 0xff) {
      i++;
      continue;
    }
    if (SOF.has(m)) return { width: be16(b, i + 7), height: be16(b, i + 5), type: 'jpeg' };
    // Маркеры без длины: RST, SOI, EOI, TEM.
    if ((m >= 0xd0 && m <= 0xd9) || m === 0x01) {
      i += 2;
      continue;
    }
    i += 2 + be16(b, i + 2);
  }
  return null;
}

function webp(b: Uint8Array): ImageSize | null {
  if (ascii(b, 12, 'VP8 ') && b.length >= 30) {
    return { width: le16(b, 26) & 0x3fff, height: le16(b, 28) & 0x3fff, type: 'webp' };
  }
  if (ascii(b, 12, 'VP8L') && b.length >= 25) {
    const width = 1 + (((b[22] & 0x3f) << 8) | b[21]);
    const height = 1 + (((b[24] & 0x0f) << 10) | (b[23] << 2) | ((b[22] & 0xc0) >> 6));
    return { width, height, type: 'webp' };
  }
  if (ascii(b, 12, 'VP8X') && b.length >= 30) return { width: 1 + le24(b, 24), height: 1 + le24(b, 27), type: 'webp' };
  return null;
}

/** Размер по заголовку; `null` — не PNG, JPEG или WebP либо заголовок испорчен. */
export function imageSize(b: Uint8Array): ImageSize | null {
  if (b.length >= 24 && b[0] === 0x89 && ascii(b, 1, 'PNG') && ascii(b, 12, 'IHDR')) {
    return { width: be32(b, 16), height: be32(b, 20), type: 'png' };
  }
  if (b.length >= 4 && b[0] === 0xff && b[1] === 0xd8) return jpeg(b);
  if (b.length >= 16 && ascii(b, 0, 'RIFF') && ascii(b, 8, 'WEBP')) return webp(b);
  return null;
}
