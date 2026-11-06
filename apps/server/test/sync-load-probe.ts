// Отдельный процесс нагрузки на настоящий сервер (спека подсистемы 4, раздел 13.1): клиенты — дочерний процесс, поэтому
// память процесса — только память сервера. Печатает итоги в JSON.
//   memory N               — память на соединение: N подключено и подписано.
//   latency N W HZ SECS    — от кадра до рассылки на N участников: W из них пишут по HZ правок в секунду SECS секунд.
//                            Мерит сам сервер (`SyncHub.onTiming`): от прихода кадра до конца его рассылки, p50/p95/p99.
//   slow MB                — медленный читатель: не читает, пока другой пишет MB мегабайт, затем читает и получает всё по порядку.
//   slowbig MB COUNT       — то же кадрами по MB мегабайт: ни один кадр, свой ack и чужие правки, не теряется.

import { encodeClientFrame, encodeVersion, parseServerFrame, syncPath } from '@ferrum/protocol';
import { LoroDoc } from 'loro-crdt';
import { connect } from 'node:net';
import { apiStand } from './api-stand';
import { Client } from './sync-client';

const [mode, arg] = process.argv.slice(2);
const n = Number(arg);

if (mode === 'clients') {
  // Дочерний процесс: N соединений, каждое подписано; первые `writers` пишут по `hz` правок в секунду `secs` секунд.
  // Итог — на stdout: `ready`, затем JSON с тем, сколько правок дошло до каждого.
  const [url, cookie, origin, w = '0', hz = '0', secs = '0'] = process.argv.slice(4);
  const writers = Number(w);
  let ready = 0;
  const sockets: WebSocket[] = [];
  const got = Array.from({ length: n }, () => 0);
  const acked = Array.from({ length: n }, () => 0);
  for (let i = 0; i < n; i++) {
    const ws = new WebSocket(url, { headers: { cookie, origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      ws.send(encodeClientFrame({ t: 'hello', protocol: 1, schema: 1 }));
      ws.send(encodeClientFrame({ t: 'subscribe', doc: 'root', version: encodeVersion(new Map()) }));
    };
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (f?.t === 'subscribed' && ++ready === n) console.log('ready');
      if (f?.t === 'updates') got[i]++;
      if (f?.t === 'ack') acked[i]++;
    };
    sockets.push(ws);
    if (i % 50 === 49) await Bun.sleep(5);
  }
  if (writers === 0) await new Promise(() => {});
  while (ready < n) await Bun.sleep(5);
  const docs = Array.from({ length: writers }, () => new LoroDoc());
  const step = 1000 / Number(hz);
  const start = performance.now();
  let sent = 0;
  // Пишущие по очереди, равномерно: каждый — раз в `step` мс.
  for (let k = 0; performance.now() - start < Number(secs) * 1000; k++) {
    const i = k % writers;
    const d = docs[i];
    const from = d.oplogVersion();
    d.getMap('m').set('x', k);
    d.commit();
    sockets[i].send(encodeClientFrame({ t: 'update', doc: 'root', clientSeq: k, bytes: d.export({ mode: 'update', from }) }));
    sent++;
    const due = start + ((k + 1) * step) / writers;
    while (performance.now() < due) await Bun.sleep(1);
  }
  // Ждём, пока всё дойдёт: каждый получает всё, кроме своего.
  const perWriter = (i: number) => Math.floor(sent / writers) + (i < sent % writers ? 1 : 0);
  const until = performance.now() + 30_000;
  const done = () => got.every((g, i) => g === sent - (i < writers ? perWriter(i) : 0));
  while (!done() && performance.now() < until) await Bun.sleep(20);
  console.log(JSON.stringify({ sent, delivered: done(), acked: acked.reduce((a, b) => a + b, 0) }));
  await new Promise(() => {});
}

const s = await apiStand(
  {
    syncLimits: {
      connectionsPerUser: 100_000,
      userOpensBurst: 100_000,
      framesBurst: 1_000_000,
      userFramesBurst: 1_000_000,
      // Окно группы журнала для сравнения (по умолчанию `GROUP_MS`, прежнее по спеке — 20 мс): `PROBE_GROUP_MS=20 bun test/sync-load-probe.ts latency …`.
      log: process.env.PROBE_GROUP_MS ? { groupMs: Number(process.env.PROBE_GROUP_MS) } : {},
    },
  },
  true,
);
const who = await s.signIn(`load-${Date.now()}@example.com`);
const file = (await s.newFile(who)).id;
const url = s.publicUrl.replace('http', 'ws') + syncPath(file, who.id);

async function subscribed(count: number, writing: string[] = []): Promise<{ child: Bun.Subprocess; next: () => Promise<string> }> {
  const child = Bun.spawn(['bun', import.meta.path, 'clients', String(count), url, who.cookie, s.publicUrl, ...writing], {
    stdout: 'pipe',
    stderr: 'ignore',
  });
  const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
  let text = '';
  /** Следующая строка вывода детей. */
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
  return { child, next };
}

function percentile(v: number[], p: number): number {
  const sorted = [...v].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil((sorted.length * p) / 100) - 1)];
}

let out: Record<string, unknown> = {};
if (mode === 'memory') {
  Bun.gc(true);
  const before = process.memoryUsage().rss;
  const { child } = await subscribed(n);
  Bun.gc(true);
  const after = process.memoryUsage().rss;
  out = { connections: s.hub.connections(), growthMb: (after - before) / 1e6, perConnectionKb: (after - before) / n / 1000 };
  child.kill();
} else if (mode === 'latency') {
  const [, , writers, hz, secs] = process.argv.slice(2).map(Number);
  const times: number[] = [];
  s.hub.onTiming = (ms) => times.push(ms);
  const { child, next } = await subscribed(n, [String(writers), String(hz), String(secs)]);
  const report = JSON.parse(await next()) as { sent: number; delivered: boolean; acked: number };
  s.hub.onTiming = null;
  out = {
    participants: n,
    writers,
    hz,
    measured: times.length,
    ...report,
    p50: percentile(times, 50),
    p95: percentile(times, 95),
    p99: percentile(times, 99),
    max: percentile(times, 100),
  };
  child.kill();
} else if (mode === 'slow' || mode === 'slowbig') {
  const big = mode === 'slowbig';
  const frameMb = big ? n : 1;
  const count = big ? Number(process.argv[4]) : n;
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
  const blob = 'x'.repeat(frameMb * 1024 * 1024);
  let own = 0;
  for (let i = 0; i < count; i++) {
    const from = d.oplogVersion();
    d.getMap('m').set('b', `${i}${blob}`);
    d.commit();
    writer.update('root', i, d.export({ mode: 'update', from }));
    await writer.wait('ack', (f) => f.clientSeq === i, 60_000);
    writer.frames.length = 0;
    Bun.gc(false);
    if (big && i === 1) {
      // Свою правку читатель шлёт, когда его сокет уже полон: `ack` к нему должен дойти.
      const mine = new LoroDoc();
      const f0 = mine.oplogVersion();
      mine.getMap('own').set('y', 1);
      mine.commit();
      const p = encodeClientFrame({ t: 'update', doc: 'root', clientSeq: 777, bytes: mine.export({ mode: 'update', from: f0 }) });
      raw.write(mask(p));
      own = 1;
      await Bun.sleep(300);
    }
  }
  clearInterval(probe);
  // Читает всё: сервер догоняет из базы, по порядку, без пропусков.
  raw.resume();
  const seqs: number[] = [];
  let acks = 0;
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
        if (f?.t === 'ack') acks++;
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
  const total = count + own;
  const until = performance.now() + 120_000;
  // Своя правка читателю не приходит: всё остальное — до последнего номера.
  while ((seqs.length < count || acks < own) && performance.now() < until) await Bun.sleep(20);
  const ownSeq = own ? [...Array(total).keys()].map((q) => q + 1).find((q) => !seqs.includes(q)) : undefined;
  const missing = [...Array(total).keys()].map((q) => q + 1).filter((q) => !seqs.includes(q) && q !== ownSeq);
  out = {
    frames: seqs.length,
    ordered: seqs.every((q, i) => i === 0 || q > seqs[i - 1]),
    missing,
    acks,
    peakBufferedMb: peak / 1e6,
    sentMb: count * frameMb,
  };
  raw.destroy();
}
console.log(JSON.stringify(out));
await s.stop();
process.exit(0);
