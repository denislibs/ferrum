// Отдельный процесс нагрузки на настоящий сервер (спека подсистемы 4, раздел 13.1): клиенты — дочерний процесс, поэтому
// память процесса — только память сервера. Печатает итоги в JSON.
//   memory N     — память на соединение: N подключено и подписано.
//   latency N    — от кадра до рассылки на N участников, p50 и p95.
//   slow MB      — медленный читатель: не читает, пока другой пишет MB мегабайт, затем читает и получает всё по порядку.

import { encodeClientFrame, encodeVersion, parseServerFrame, syncPath } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { connect } from 'node:net';
import { apiStand } from './api-stand';
import { Client } from './sync-client';

const [mode, arg] = process.argv.slice(2);
const n = Number(arg);

if (mode === 'clients') {
  // Дочерний процесс: N соединений, каждое подписано и молчит; итог — на stdout.
  const [url, cookie, origin] = process.argv.slice(4);
  let ready = 0;
  const sockets: WebSocket[] = [];
  for (let i = 0; i < n; i++) {
    const ws = new WebSocket(url, { headers: { cookie, origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      ws.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 }));
      ws.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    };
    ws.onmessage = (e) => {
      if (parseServerFrame(new Uint8Array(e.data as ArrayBuffer))?.t === 'subscribed') {
        if (++ready === n) console.log('ready');
      }
    };
    sockets.push(ws);
    if (i % 50 === 49) await Bun.sleep(5);
  }
  await new Promise(() => {});
}

const s = await apiStand({ syncLimits: { connectionsPerUser: 100_000, framesBurst: 1_000_000, userFramesBurst: 1_000_000 } }, true);
const who = await s.signIn(`load-${Date.now()}@example.com`);
const file = (await s.newFile(who)).id;
const url = s.publicUrl.replace('http', 'ws') + syncPath(file, who.id);

async function subscribed(count: number): Promise<Bun.Subprocess> {
  const child = Bun.spawn(['bun', import.meta.path, 'clients', String(count), url, who.cookie, s.publicUrl], {
    stdout: 'pipe',
    stderr: 'ignore',
  });
  const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
  for (let text = ''; !text.includes('ready');) {
    const r = await reader.read();
    if (r.done) throw new Error('клиенты упали');
    text += new TextDecoder().decode(r.value);
  }
  return child;
}

function percentile(v: number[], p: number): number {
  const sorted = [...v].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil((sorted.length * p) / 100) - 1)];
}

let out: Record<string, unknown> = {};
if (mode === 'memory') {
  Bun.gc(true);
  const before = process.memoryUsage().rss;
  const child = await subscribed(n);
  Bun.gc(true);
  const after = process.memoryUsage().rss;
  out = { connections: s.hub.connections(), growthMb: (after - before) / 1e6, perConnectionKb: (after - before) / n / 1000 };
  child.kill();
} else if (mode === 'latency') {
  const child = await subscribed(n);
  const writer = (await Client.open(s, who, file))!;
  writer.hello();
  writer.subscribe('root');
  await writer.wait('subscribed');
  const listener = (await Client.open(s, who, file))!;
  listener.hello();
  listener.subscribe('root');
  await listener.wait('subscribed');
  const d = new LoroDoc();
  const times: number[] = [];
  for (let i = 0; i < 200; i++) {
    const from = d.oplogVersion();
    d.getMap('m').set('x', i);
    d.commit();
    const bytes = d.export({ mode: 'update', from });
    const seen = listener.frames.length;
    const t = performance.now();
    writer.update('root', i, bytes);
    while (listener.frames.length === seen) await Bun.sleep(0);
    times.push(performance.now() - t);
    await Bun.sleep(5);
  }
  out = { participants: n, p50: percentile(times, 50), p95: percentile(times, 95), max: percentile(times, 100) };
  child.kill();
} else if (mode === 'slow') {
  const writer = (await Client.open(s, who, file))!;
  writer.hello();
  writer.subscribe('root');
  await writer.wait('subscribed');
  // Настоящий сокет, который не читает: ядро и буфер сервера заполняются, как у зависшего клиента.
  const raw = connect(new URL(s.publicUrl).port as unknown as number, '127.0.0.1');
  let chunks: Buffer[] = [];
  let handshaken: () => void;
  const up = new Promise<void>((r) => (handshaken = r));
  raw.on('data', (b: Buffer) => {
    chunks.push(b);
    if (Buffer.concat(chunks).includes('\r\n\r\n')) handshaken();
  });
  raw.write(
    `GET ${syncPath(file, who.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${who.cookie}\r\n\r\n`,
  );
  await up;
  chunks = [Buffer.concat(chunks)];
  const mask = (p: Uint8Array): Buffer => {
    const head =
      p.length < 126
        ? Buffer.from([0x82, 0x80 | p.length])
        : Buffer.concat([Buffer.from([0x82, 0x80 | 126]), Buffer.from([p.length >> 8, p.length & 255])]);
    return Buffer.concat([head, Buffer.from([0, 0, 0, 0]), p]);
  };
  raw.write(mask(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 })));
  raw.write(mask(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) })));
  await Bun.sleep(300);
  raw.pause();
  let peak = 0;
  const probe = setInterval(() => (peak = Math.max(peak, s.hub.bufferedBytes())), 5);
  const d = new LoroDoc();
  const blob = 'x'.repeat(1024 * 1024);
  for (let i = 0; i < n; i++) {
    const from = d.oplogVersion();
    d.getMap('m').set('b', `${i}${blob}`);
    d.commit();
    writer.update('root', i, d.export({ mode: 'update', from }));
    await writer.wait('ack', (f) => f.clientSeq === i, 30_000);
    writer.frames.length = 0;
    Bun.gc(false);
  }
  clearInterval(probe);
  // Читает всё: сервер догоняет из базы, по порядку, без пропусков.
  raw.resume();
  const seqs: number[] = [];
  let buf = Buffer.concat(chunks);
  const skip = buf.indexOf('\r\n\r\n') + 4;
  buf = buf.subarray(skip);
  chunks = [];
  const parse = () => {
    for (;;) {
      if (buf.length < 2) return;
      const code = buf[1] & 127;
      const headLen = code === 126 ? 4 : code === 127 ? 10 : 2;
      if (buf.length < headLen) return;
      const len = code === 126 ? buf.readUInt16BE(2) : code === 127 ? Number(buf.readBigUInt64BE(2)) : code;
      if (buf.length < headLen + len) return;
      const payload = buf.subarray(headLen, headLen + len);
      if ((buf[0] & 15) === 2) {
        const f = parseServerFrame(new Uint8Array(payload));
        if (f?.t === 'updates') seqs.push(f.seq);
      }
      buf = buf.subarray(headLen + len);
    }
  };
  raw.removeAllListeners('data');
  raw.on('data', (b: Buffer) => {
    buf = Buffer.concat([buf, b]);
    parse();
  });
  parse();
  const until = performance.now() + 60_000;
  while (seqs.at(-1) !== n && performance.now() < until) await Bun.sleep(20);
  out = { frames: seqs.length, ordered: seqs.every((q, i) => q === i + 1), peakBufferedMb: peak / 1e6, sentMb: n };
  raw.destroy();
}
console.log(JSON.stringify(out));
await s.stop();
process.exit(0);
