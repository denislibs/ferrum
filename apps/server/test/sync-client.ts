// Клиент WebSocket для тестов: настоящий сокет и настоящие кадры, настоящие документы Loro.

import {
  encodeClientFrame,
  encodeVersion,
  parseServerFrame,
  SYNC_PROTOCOL,
  syncPath,
  type ServerFrame,
  type VersionMap,
} from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { LoroDoc } from 'loro-crdt';
import type { ApiStand, Person } from './api-stand';

export function peers(doc: LoroDoc): VersionMap {
  return new Map([...doc.oplogVersion().toJSON()].map(([p, c]) => [String(p), c]));
}

export class Client {
  readonly frames: ServerFrame[] = [];
  closed: { code: number; reason: string } | null = null;
  private waiters: (() => void)[] = [];
  private constructor(readonly ws: WebSocket) {
    ws.binaryType = 'arraybuffer';
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (!f) throw new Error('неверный кадр сервера');
      this.frames.push(f);
      this.wake();
    };
    ws.onclose = (e) => {
      this.closed = { code: e.code, reason: e.reason };
      this.wake();
    };
  }

  private wake(): void {
    for (const w of this.waiters.splice(0)) w();
  }

  /** Подключение; отказ при открытии — `null`. */
  static async open(
    s: ApiStand,
    who: Person,
    file: string,
    over: { origin?: string; user?: string; cookie?: string } = {},
  ): Promise<Client | null> {
    const url = s.publicUrl.replace('http', 'ws') + syncPath(file, over.user ?? who.id);
    const ws = new WebSocket(url, { headers: { origin: over.origin ?? s.publicUrl, cookie: over.cookie ?? who.cookie } } as never);
    return new Promise((resolve) => {
      ws.onopen = () => resolve(new Client(ws));
      ws.onerror = () => resolve(null);
    });
  }

  send(bytes: Uint8Array): void {
    this.ws.send(bytes);
  }

  hello(protocol = SYNC_PROTOCOL): void {
    this.send(encodeClientFrame({ t: 'hello', protocol, schema: 1 }));
  }

  subscribe(doc: string, version: VersionMap = new Map()): void {
    this.send(encodeClientFrame({ t: 'subscribe', doc, version: encodeVersion(version) }));
  }

  update(doc: string, clientSeq: number, bytes: Uint8Array): void {
    this.send(encodeClientFrame({ t: 'update', doc, clientSeq, bytes }));
  }

  /** Кадр, подходящий условию; ждать — до срока теста. */
  async wait<T extends ServerFrame['t']>(
    t: T,
    match: (f: Extract<ServerFrame, { t: T }>) => boolean = () => true,
    ms = 10_000,
  ): Promise<Extract<ServerFrame, { t: T }>> {
    const deadline = performance.now() + ms;
    for (;;) {
      const f = this.frames.find((x) => x.t === t && match(x as Extract<ServerFrame, { t: T }>));
      if (f) return f as Extract<ServerFrame, { t: T }>;
      if (performance.now() > deadline)
        throw new Error(`нет кадра ${t}: ${JSON.stringify(this.frames.map((x) => x.t))}, закрыт: ${JSON.stringify(this.closed)}`);
      await this.tick(deadline);
    }
  }

  async waitClosed(ms = 10_000): Promise<{ code: number; reason: string }> {
    const deadline = performance.now() + ms;
    while (!this.closed) {
      if (performance.now() > deadline) throw new Error('соединение не закрыто');
      await this.tick(deadline);
    }
    return this.closed;
  }

  private tick(deadline: number): Promise<void> {
    return new Promise((resolve) => {
      const t = setTimeout(resolve, Math.max(1, Math.min(200, deadline - performance.now())));
      this.waiters.push(() => {
        clearTimeout(t);
        resolve();
      });
    });
  }

  close(): void {
    this.ws.close();
  }
}

/** Файл с настоящим снимком: корень — снимок документа `base`. */
export function realContainer(root: Uint8Array, pages: [string, Uint8Array][] = []): Uint8Array<ArrayBuffer> {
  return buildContainer([[1, '', root], ...pages.map(([id, b]): [number, string, Uint8Array] => [2, id, b])]);
}

export function buildContainer(sections: [number, string, Uint8Array][]): Uint8Array<ArrayBuffer> {
  const parts: Uint8Array[] = [];
  const head = new Uint8Array(16);
  head.set([70, 69, 82, 82, 85, 77, 0, 0]);
  new DataView(head.buffer).setUint32(8, 1, true);
  new DataView(head.buffer).setUint32(12, sections.length, true);
  parts.push(head);
  for (const [kind, id, bytes] of sections) {
    const idb = new TextEncoder().encode(id);
    const out = new Uint8Array(5 + idb.length + 40 + bytes.length);
    const v = new DataView(out.buffer);
    out[0] = kind;
    v.setUint32(1, idb.length, true);
    out.set(idb, 5);
    v.setBigUint64(5 + idb.length, BigInt(bytes.length), true);
    // Контрольная сумма — как у ядра: копия секции при сжатии должна открываться им же.
    out.set(blake3(bytes), 13 + idb.length);
    out.set(bytes, 45 + idb.length);
    parts.push(out);
  }
  const total = parts.reduce((n, p) => n + p.length, 0);
  const all = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    all.set(p, at);
    at += p.length;
  }
  return all;
}

export const newDoc = (seedPeer?: bigint): LoroDoc => {
  const d = new LoroDoc();
  if (seedPeer !== undefined) d.setPeerId(seedPeer as never);
  return d;
};
