// Отдельный процесс для теста памяти: настоящий сервер и загрузка потоком, без шума других тестов.
// Клиент не держит файл в памяти: куски — один и тот же буфер. Печатает прирост памяти процесса в МБ.

import { containerHeader } from '@ferrum/protocol';
import { apiStand } from './api-stand';

const MB = 1024 * 1024;
const SIZE = Number(process.argv[2]) * MB;
const s = await apiStand({}, true);
const who = await s.signIn(`memory-${Date.now()}@example.com`);
const f = await s.newFile(who);
const head = new Uint8Array(16 + 5 + 40);
head.set(containerHeader(1));
head[16] = 1;
new DataView(head.buffer).setBigUint64(16 + 5, BigInt(SIZE), true);
const chunk = new Uint8Array(256 * 1024);
let left = SIZE;
let sentHead = false;
const body = new ReadableStream<Uint8Array>({
  pull(c) {
    if (!sentHead) {
      sentHead = true;
      c.enqueue(head);
    } else if (left === 0) c.close();
    else {
      const n = Math.min(left, chunk.length);
      left -= n;
      c.enqueue(chunk.subarray(0, n));
    }
  },
});
Bun.gc(true);
const before = process.memoryUsage().rss;
let peak = before;
const probe = setInterval(() => (peak = Math.max(peak, process.memoryUsage().rss)), 5);
const put = await fetch(`${s.publicUrl}/api/files/${f.id}/content`, {
  method: 'PUT',
  headers: { origin: s.publicUrl, cookie: who.cookie, 'if-match': '"0"', 'x-ferrum-write': crypto.randomUUID() },
  body,
});
const got = await s.call(who, 'GET', `/api/files/${f.id}/content`);
let read = 0;
const reader = got.body!.getReader();
for (let r = await reader.read(); !r.done; r = await reader.read()) read += r.value.length;
clearInterval(probe);
console.log(JSON.stringify({ status: put.status, read, growthMb: (peak - before) / MB }));
await s.stop();
process.exit(0);
