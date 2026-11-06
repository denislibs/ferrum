// Нагрузка присутствия отдельным процессом (спека подсистемы 4, разделы 9 и 13.1): клиенты — дочерний процесс, поэтому
// процессор и память процесса — только сервера. Печатает итоги в JSON.
//   load N HZ SECS [hostile] — N участников, каждый шлёт курсор HZ раз в секунду SECS секунд. Задержка — от отправки до
//                              прихода к другому участнику (часы одного процесса клиентов). `hostile` — ещё один
//                              участник шлёт кадры по 2 КБ без остановки, а один сокет не читает вовсе.

import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBody,
  parseServerFrame,
  splitSelection,
  SYNC_PROTOCOL,
  syncPath,
  type PresenceState,
} from '@ferrum/protocol';
import { connect } from 'node:net';
import { apiStand } from './api-stand';

const [mode] = process.argv.slice(2);
const page = '3@7910247476534078275';
// Часы процесса клиентов: и отправка, и приём — в нём.
const clock = () => performance.now();

if (mode === 'clients') {
  const [n, hz, secs, url, cookie, origin, hostile] = process.argv.slice(3);
  const count = Number(n);
  const sockets: WebSocket[] = [];
  const lat: number[] = [];
  let ready = 0;
  let received = 0;
  for (let i = 0; i < count; i++) {
    const ws = new WebSocket(url, { headers: { cookie, origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => ws.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 }));
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (f?.t === 'role' && ++ready === count) console.log('ready');
      if (f?.t !== 'presence') return;
      const now = clock();
      for (const it of f.items) {
        if (it.k !== 'state' || !it.state.cursor) continue;
        received++;
        // Курсор x — момент отправки.
        if (it.state.cursor[1] === 1) lat.push(now - it.state.cursor[0]);
      }
    };
    sockets.push(ws);
    if (i % 25 === 24) await Bun.sleep(5);
  }
  while (ready < count) await Bun.sleep(5);
  // У каждого — выделение из 1000 узлов, как у перетаскивающего.
  const ids = Array.from({ length: 1000 }, (_, i) => `${10 + i * 12}@7910247476534078275`);
  const parts = splitSelection(ids);
  const selection = { rev: 1, count: ids.length, parts: parts.length, bounds: [0, 0, 10, 10] as [number, number, number, number] };
  let flood: ReturnType<typeof setInterval> | null = null;
  if (hostile === 'hostile') {
    const big = encodePresenceBody({
      page,
      view: [0, 0, 1, 1],
      cursor: [0, 2],
      selection: { rev: 9, count: 1, parts: 16, bounds: [0, 0, 1, 1] },
      gesture: null,
    });
    const huge = new Uint8Array(2040);
    huge.set(big);
    const evil = sockets[0];
    // Без остановки: ~4000 кадров в секунду, верных и неверных вперемешку.
    flood = setInterval(() => {
      for (let k = 0; k < 40; k++) {
        evil.send(encodeClientFrame({ t: 'presence', body: big }));
        evil.send(encodeClientFrame({ t: 'presence', body: huge }));
      }
    }, 10);
  }
  const step = 1000 / Number(hz);
  const start = performance.now();
  let sent = 0;
  for (let k = 0; performance.now() - start < Number(secs) * 1000; k++) {
    const t0 = performance.now();
    for (const [i, ws] of sockets.entries()) {
      if (hostile === 'hostile' && i === 0) continue;
      const st: PresenceState = {
        page,
        view: [0, 0, 1920, 1080],
        cursor: [clock(), 1],
        selection,
        gesture: k % 4 === 0 ? { kind: 'move', transform: [1, 0, 0, 1, k, k], frame: [1, 0, 0, 1, 0, 0], k: 1 } : null,
      };
      ws.send(encodeClientFrame({ t: 'presence', body: encodePresenceBody(st) }));
      sent++;
      // Части выделения — за первым состоянием, как у клиента.
      if (k === 0)
        parts.forEach((bytes, index) =>
          ws.send(encodeClientFrame({ t: 'presence-part', body: encodePartBody({ rev: 1, index, parts: parts.length, bytes }) })),
        );
    }
    const wait = step - (performance.now() - t0);
    if (wait > 0) await Bun.sleep(wait);
  }
  if (flood) clearInterval(flood);
  await Bun.sleep(500);
  const sorted = lat.sort((a, b) => a - b);
  const at = (p: number) => sorted[Math.min(sorted.length - 1, Math.ceil((sorted.length * p) / 100) - 1)] ?? -1;
  console.log(JSON.stringify({ sent, received, samples: sorted.length, p50: at(50), p95: at(95), p99: at(99), max: at(100) }));
  // Родитель убивает процесс клиентов; если он упал раньше — выход сам.
  setTimeout(() => process.exit(0), 10_000);
  await new Promise(() => {});
}

// Нечего ждать: дочерний процесс клиентов без родителя не живёт.
const s = await apiStand({
  syncLimits: {
    connectionsPerUser: 100_000,
    userOpensBurst: 100_000,
    // 50 участников — соединения одного человека: пределы человека сняты, предел файла (100) — как есть.
    presence: { userFramesPerSecond: 100_000, userFramesBurst: 100_000, userConnections: 100_000 },
  },
});
const who = await s.signIn(`presence-load-${Date.now()}@example.com`);
const file = (await s.newFile(who)).id;
const url = s.publicUrl.replace('http', 'ws') + syncPath(file, who.id);
const [, n, hz, secs, hostile = ''] = process.argv.slice(2);

// Сокет, который не читает: буфер сервера для него не должен расти.
let raw: ReturnType<typeof connect> | null = null;
if (hostile === 'hostile') {
  raw = connect(Number(new URL(s.publicUrl).port), '127.0.0.1');
  raw.write(
    `GET ${syncPath(file, who.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${who.cookie}\r\n\r\n`,
  );
  await Bun.sleep(200);
  const hello = encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
  raw.write(Buffer.concat([Buffer.from([0x82, 0x80 | hello.length, 0, 0, 0, 0]), hello]));
  await Bun.sleep(200);
  raw.pause();
}

Bun.gc(true);
const rss0 = process.memoryUsage().rss;
const heap0 = process.memoryUsage().heapUsed;
const child = Bun.spawn(['bun', import.meta.path, 'clients', n, hz, secs, url, who.cookie, s.publicUrl, hostile], {
  stdout: 'pipe',
  stderr: 'inherit',
});
const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
let text = '';
const next = async (): Promise<string> => {
  for (;;) {
    const at = text.indexOf('\n');
    if (at >= 0) {
      const line = text.slice(0, at);
      text = text.slice(at + 1);
      return line;
    }
    const r = await reader.read();
    if (r.done) throw new Error('клиенты упали');
    text += new TextDecoder().decode(r.value);
  }
};
if ((await next()) !== 'ready') throw new Error('клиенты не готовы');
const cpu0 = process.cpuUsage();
const t0 = performance.now();
let peakBuffered = 0;
let peakHeld = 0;
let peakRss = 0;
const probe = setInterval(() => {
  peakBuffered = Math.max(peakBuffered, s.hub.bufferedBytes());
  peakHeld = Math.max(peakHeld, s.hub.presence.heldBytes());
  peakRss = Math.max(peakRss, process.memoryUsage().rss);
}, 20);
const report = JSON.parse(await next()) as Record<string, number>;
clearInterval(probe);
const cpu = process.cpuUsage(cpu0);
const wall = performance.now() - t0;
Bun.gc(true);
const rssAfter = process.memoryUsage().rss;
// Живая куча после сборки мусора, пока участники на месте: RSS не возвращает страницы системе и показывает пик.
const heapAfter = process.memoryUsage().heapUsed;
console.log(
  JSON.stringify({
    participants: Number(n),
    hz: Number(hz),
    ...report,
    serverCpuPercent: ((cpu.user + cpu.system) / 1000 / wall) * 100,
    batches: s.hub.presence.sent,
    deferred: s.hub.presence.deferred,
    dropped: s.hub.presence.dropped,
    rssPeakGrowthMb: (peakRss - rss0) / 1e6,
    rssAfterGcGrowthMb: (rssAfter - rss0) / 1e6,
    heapGrowthMb: (heapAfter - heap0) / 1e6,
    peakBufferedKb: peakBuffered / 1024,
    peakHeldKb: peakHeld / 1024,
  }),
);
child.kill();
raw?.destroy();
await s.stop();
process.exit(0);
