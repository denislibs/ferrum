// Значок приложения до своего набора: квадрат цвета `oxide` дизайн-системы с тёмной
// рамкой фрейма — PNG 1024 × 1024 RGBA (Tauri требует RGBA). Пишется без зависимостей
// (zlib из Node). Остальные размеры и .icns делает `npx tauri icon` из этого файла.
import { writeFileSync, mkdirSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const SIZE = 1024;
const OXIDE = [0xf2, 0x81, 0x4a, 0xff];
const INK = [0x1a, 0x19, 0x18, 0xff];
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../src-tauri/icons/icon.png');

const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // фильтр строки: нет
  for (let x = 0; x < SIZE; x++) {
    // Рамка фрейма: полоса 48 px на отступе 240 px от края.
    const inFrame = x >= 240 && x < SIZE - 240 && y >= 240 && y < SIZE - 240;
    const inHole = x >= 288 && x < SIZE - 288 && y >= 288 && y < SIZE - 288;
    const c = inFrame && !inHole ? INK : OXIDE;
    raw.set(c, y * (SIZE * 4 + 1) + 1 + x * 4);
  }
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // бит на канал
ihdr[9] = 6; // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw)),
  chunk('IEND', Buffer.alloc(0)),
]);
mkdirSync(path.dirname(out), { recursive: true });
writeFileSync(out, png);
console.log(`значок: ${out}`);
