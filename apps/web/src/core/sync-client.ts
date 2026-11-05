// Соединение файла со стороны клиента (спека подсистемы 4, разделы 7 и 8.1). Логика без DOM и без настоящего сокета: её запускает
// `worker/sync.worker.ts`, модульные тесты дают ей свой сокет с настоящим `bufferedAmount`. Основной поток трафика не видит:
// правки идут из worker'а ядра прямым `MessagePort`.
//
// Ничего не хранится здесь надолго. Что отправить, знает курсор ядра: на каждом новом соединении ядро подписывается своей версией,
// сервер отвечает своей, и ядро шлёт всё сверх неё. Поэтому обрыв, рестарт сервера, отказ «повторите» и пауза — один и тот же
// путь (`resync`), а неподтверждённое не теряется: оно в документе и в журнале браузера.

import {
  encodeClientFrame,
  parseServerFrame,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  syncPath,
  type ServerFrame,
  type SyncRefusal,
} from '@ferrum/protocol';
import type { AccessState, Clock } from './server-files';

/** `offline` и `saving` — тексты индикатора §8.1. `rejected` — правки больше не уходят, человек должен знать. */
export type SyncStatus = 'connecting' | 'synced' | 'saving' | 'offline' | 'paused' | 'rejected';

export interface SocketLike {
  binaryType: string;
  readonly bufferedAmount: number;
  onopen: ((e: unknown) => void) | null;
  onmessage: ((e: { data: unknown }) => void) | null;
  onclose: ((e: { code: number }) => void) | null;
  onerror: ((e: unknown) => void) | null;
  send(bytes: Uint8Array): void;
  close(code?: number): void;
}

/** Команды ядру: `transfer` — буферы уходят без копии. `applied` — ядро закончило `applyRemote`. */
export type CoreCommand =
  | { type: 'syncStart' }
  | { type: 'syncReset' }
  | { type: 'syncSubscribed'; doc: string; version: Uint8Array }
  | { type: 'syncAcked'; doc: string; clientSeq: number }
  | { type: 'applyRemote'; doc: string; batches: Uint8Array[] }
  | { type: 'addRemoteImage'; hash: string; bytes: Uint8Array };

/** События ядра, которые идут этому классу. */
export type CoreSyncEvent =
  | { type: 'syncSubscribe'; doc: string; version: Uint8Array }
  | { type: 'localImage'; hash: string; bytes: Uint8Array }
  | { type: 'localUpdates'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { type: 'syncState'; unacked: boolean }
  | { type: 'imagesNeeded'; hashes: string[] };

export interface SyncEnv {
  file: string;
  user: string;
  schema: number;
  /** Адрес сайта: `https://…` → `wss://…`. */
  origin: string;
  socket(url: string): SocketLike;
  core: {
    command(c: CoreCommand): void;
    /** Отдавать ли правки: ядро шлёт, только пока есть куда. */
    flush(on: boolean): void;
  };
  files: {
    uploadImage(hash: string, bytes: Uint8Array): Promise<void>;
    downloadImage(hash: string): Promise<ArrayBuffer | null>;
    snapshot(path: string): Promise<ArrayBuffer>;
    /** Лёгкая проверка: почему соединение не открывается. Сеть — `null`. */
    access(): Promise<AccessState | null>;
  };
  clock: Clock;
  random(): number;
  status(s: SyncStatus, detail?: string): void;
  access(state: AccessState): void;
  reload(): void;
}

/** Переподключение: от 0,5 до 30 с с разбросом, сразу при `online` (спека, раздел 8.1). */
export const RECONNECT_MIN_MS = 500;
export const RECONNECT_MAX_MS = 30_000;
/** Сокет молчит столько после `ping` — мёртв: браузер об этом не сообщает. */
export const PING_EVERY_MS = 20_000;
export const PONG_WAIT_MS = 15_000;
/** В буфере сокета больше этого — следующая пачка ждёт: сокет не копит без конца. */
const SEND_HIGH_WATER = 1024 * 1024;
const SEND_RETRY_MS = 25;
/** Столько байт правок может ждать отправки здесь. */
const MAX_OUT_BYTES = 32 * 1024 * 1024;
/** Загрузка картинки не удалась по сети: снова через столько, правки за ней ждут. */
const IMAGE_RETRY_MS = 5000;
/** Отказ «повторите» (`UNAVAILABLE`, `GAP`): от этого и с тем же ростом, что переподключение. */
const RESYNC_MIN_MS = 1000;
/** Столько раз подряд `GAP` без единого `ack`: клиент не может послать правки в верном порядке. */
const MAX_GAPS = 6;
/** Столько снимков подряд на документ без ответа `subscribed`. */
const MAX_SNAPSHOT_ROUNDS = 3;
/** Сколько картинок качается разом. */
const IMAGE_CONCURRENCY = 4;
/** Журнал файла полон: ждём, пока сервер его сожмёт. */
const LOG_FULL_WAIT_MS = 30_000;

type Out = { kind: 'image'; hash: string; bytes: Uint8Array } | { kind: 'update'; doc: string; clientSeq: number; bytes: Uint8Array };
type In = { kind: 'apply'; doc: string; batches: Uint8Array[] } | { kind: 'cmd'; cmd: CoreCommand };

export class SyncClient {
  private ws: SocketLike | null = null;
  private attempt = 0;
  private reconnectTimer: unknown = null;
  private pingTimer: unknown = null;
  private pongTimer: unknown = null;
  private pauseTimer: unknown = null;
  private retryTimer: unknown = null;
  private sendTimer: unknown = null;
  private opened = false;
  private ended = false;
  private rejected: string | null = null;
  private paused = false;
  private unacked = false;
  private readyDocs = 0;
  /** Подписки без ответа: пока они есть, файл ещё догоняет. */
  private pendingSubs = 0;
  private gaps = 0;
  private resyncs = 0;
  private readonly out: Out[] = [];
  private outBytes = 0;
  private sending = false;
  private readonly inbound: In[] = [];
  private applying = false;
  private snapshots = 0;
  private readonly snapshotRounds = new Map<string, number>();
  private needImages = new Set<string>();
  private imagesActive = 0;
  private lastStatus: string | null = null;
  private generation = 0;
  /** Соединение было и пропало или не открылось: с этого момента — «нет сети», а не «подключается». */
  private lostOnce = false;
  /** С начала подписки до первого «всё на месте». */
  private catchingUp = true;

  constructor(private readonly env: SyncEnv) {}

  start(): void {
    this.env.core.command({ type: 'syncStart' });
    this.emitStatus();
    this.connect();
  }

  /** Вкладка ушла с файла. */
  stop(): void {
    this.ended = true;
    this.clearTimers();
    this.env.core.flush(false);
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onclose = null;
      ws.close();
    }
  }

  /** Браузер сообщил, что сеть вернулась: не ждать таймера. */
  online(): void {
    if (this.ended || this.ws) return;
    this.attempt = 0;
    this.clock(this.reconnectTimer);
    this.reconnectTimer = null;
    this.connect();
  }

  /** Сколько раз заново подписывались, для тестов. */
  get resubscribes(): number {
    return this.resyncs;
  }

  private clock(t: unknown): void {
    if (t !== null) this.env.clock.clearTimeout(t);
  }

  private clearTimers(): void {
    for (const t of [this.reconnectTimer, this.pingTimer, this.pongTimer, this.pauseTimer, this.retryTimer, this.sendTimer]) this.clock(t);
    this.reconnectTimer = this.pingTimer = this.pongTimer = this.pauseTimer = this.retryTimer = this.sendTimer = null;
  }

  private connect(): void {
    if (this.ended || this.ws) return;
    const generation = ++this.generation;
    const url = this.env.origin.replace(/^http/, 'ws') + syncPath(this.env.file, this.env.user);
    let ws: SocketLike;
    try {
      ws = this.env.socket(url);
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.ws = ws;
    this.opened = false;
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      if (generation === this.generation) this.onOpen();
    };
    ws.onmessage = (e) => {
      if (generation === this.generation) this.onMessage(e.data);
    };
    ws.onclose = (e) => {
      if (generation === this.generation) this.onClose(e.code);
    };
    ws.onerror = () => {};
  }

  private onOpen(): void {
    this.opened = true;
    this.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: this.env.schema }));
    this.resync();
    this.armPing();
  }

  private send(bytes: Uint8Array): void {
    try {
      this.ws?.send(bytes);
    } catch {
      // Сокет закрывается: `onclose` уже в пути.
    }
  }

  /** Всё заново от версии сервера: новое соединение, отказ «повторите», конец паузы. Очередь правок уже не нужна: их заново соберёт
   * ядро. Картинки остаются: их байты больше нигде не собрать. */
  private resync(): void {
    this.resyncs++;
    this.readyDocs = 0;
    this.pendingSubs = 0;
    this.catchingUp = true;
    this.drop('update');
    this.enqueueCmd({ type: 'syncReset' });
    this.updateFlush();
    this.emitStatus();
  }

  private drop(kind: Out['kind']): void {
    for (let i = this.out.length - 1; i >= 0; i--) {
      if (this.out[i].kind === kind) this.outBytes -= this.out.splice(i, 1)[0].bytes.length;
    }
  }

  private push(item: Out): void {
    this.out.push(item);
    this.outBytes += item.bytes.length;
    if (this.outBytes >= MAX_OUT_BYTES) this.updateFlush();
    this.pumpOut();
  }

  private shiftOut(): void {
    const was = this.outBytes;
    this.outBytes -= this.out.shift()!.bytes.length;
    if (was >= MAX_OUT_BYTES && this.outBytes < MAX_OUT_BYTES) this.updateFlush();
  }

  private armPing(): void {
    this.clock(this.pingTimer);
    this.pingTimer = this.env.clock.setTimeout(() => {
      this.pingTimer = null;
      this.send(encodeClientFrame({ t: 'ping' }));
      this.clock(this.pongTimer);
      this.pongTimer = this.env.clock.setTimeout(() => {
        this.pongTimer = null;
        // Сокет мёртв: закрыть и не ждать `onclose` браузера, он может идти минуты.
        const ws = this.ws;
        if (ws) {
          ws.onclose = null;
          ws.close();
          this.onClose(1006);
        }
      }, PONG_WAIT_MS);
    }, PING_EVERY_MS);
  }

  private onMessage(data: unknown): void {
    if (!(data instanceof ArrayBuffer)) return;
    // Любой кадр — соединение живо.
    this.clock(this.pongTimer);
    this.pongTimer = null;
    if (!this.pingTimer) this.armPing();
    const frame = parseServerFrame(new Uint8Array(data));
    if (!frame) {
      // Сеть не доверенная: как обрыв.
      this.dropSocket();
      return;
    }
    this.handle(frame);
  }

  private dropSocket(): void {
    const ws = this.ws;
    if (ws) {
      ws.onclose = null;
      ws.close();
      this.onClose(1006);
    }
  }

  private handle(f: ServerFrame): void {
    switch (f.t) {
      case 'role':
        this.env.access({ kind: 'role', role: f.role, version: f.version, canShare: f.canShare, own: true });
        break;
      case 'subscribed':
        this.attempt = 0;
        this.lostOnce = false;
        this.snapshotRounds.delete(f.doc);
        this.readyDocs++;
        this.pendingSubs = Math.max(0, this.pendingSubs - 1);
        this.enqueueCmd({ type: 'syncSubscribed', doc: f.doc, version: f.version.slice() });
        this.emitStatus();
        break;
      case 'updates':
        this.enqueueApply(f.doc, f.bytes.slice());
        break;
      case 'ack':
        this.gaps = 0;
        this.enqueueCmd({ type: 'syncAcked', doc: f.doc, clientSeq: f.clientSeq });
        break;
      case 'refused':
        this.refused(f.code);
        break;
      case 'pause':
        this.pause(f.ms);
        break;
      case 'snapshot-needed':
        this.pendingSubs = Math.max(0, this.pendingSubs - 1);
        void this.fetchSnapshot(f.doc, f.url);
        break;
      case 'access-revoked':
        this.end();
        this.env.access({ kind: f.reason === 'session' ? 'session' : 'none' });
        break;
      case 'please-reload':
        this.end();
        this.env.reload();
        break;
      case 'pong':
        break;
    }
  }

  private end(): void {
    this.ended = true;
    this.clearTimers();
    this.env.core.flush(false);
  }

  private refused(code: SyncRefusal): void {
    switch (code) {
      case 'GAP':
        if (++this.gaps >= MAX_GAPS) {
          this.reject(
            'Сервер не принимает правки в этом порядке. Они сохранены только в этом браузере — сохраните копию на диск и обновите страницу',
          );
          return;
        }
        this.resyncLater();
        break;
      case 'UNAVAILABLE':
        this.resyncLater();
        break;
      case 'LOG_FULL':
        this.pause(LOG_FULL_WAIT_MS, 'Журнал файла на сервере переполнен. Правки ждут здесь, они не потеряются');
        break;
      case 'FORBIDDEN':
        // Роль сменилась: настоящее состояние скажет сервер, редактор перейдёт в просмотр и покажет, что правки не дошли.
        void this.env.files.access().then((a) => {
          if (a) this.env.access(a);
        });
        break;
      case 'BAD_UPDATE':
      case 'FOREIGN_PEER':
        this.reject('Сервер не принял правки. Они сохранены только в этом браузере — сохраните копию на диск и обновите страницу');
        break;
    }
  }

  /** Отказ, который повтор не исправит: правки дальше не уходят, чужие продолжают приходить, человек об этом знает. */
  private reject(message: string): void {
    this.rejected = message;
    this.env.core.flush(false);
    this.emitStatus();
  }

  private resyncLater(): void {
    if (this.retryTimer !== null || this.paused) return;
    const wait = RESYNC_MIN_MS * (0.5 + this.env.random() / 2);
    this.env.core.flush(false);
    this.retryTimer = this.env.clock.setTimeout(() => {
      this.retryTimer = null;
      if (this.ws && this.opened && !this.paused) this.resync();
    }, wait);
  }

  /** Предел частоты: пауза, а не «Нет связи». Ни один срок не считает её простоем. */
  private pause(ms: number, detail?: string): void {
    this.paused = true;
    this.env.core.flush(false);
    this.clock(this.retryTimer);
    this.retryTimer = null;
    this.clock(this.pauseTimer);
    this.pauseTimer = this.env.clock.setTimeout(() => {
      this.pauseTimer = null;
      this.paused = false;
      this.emitStatus();
      if (this.ws && this.opened) this.resync();
    }, ms);
    this.emitStatus(detail);
  }

  private onClose(code: number): void {
    this.lostOnce = true;
    this.ws = null;
    this.opened = false;
    this.readyDocs = 0;
    this.pendingSubs = 0;
    this.clock(this.pingTimer);
    this.clock(this.pongTimer);
    this.pingTimer = this.pongTimer = null;
    this.env.core.flush(false);
    this.drop('update');
    if (code === SYNC_CLOSE.SESSION) {
      this.end();
      this.env.access({ kind: 'session' });
      return;
    }
    if (code === SYNC_CLOSE.REVOKED) {
      this.end();
      this.env.access({ kind: 'none' });
      return;
    }
    if (code === SYNC_CLOSE.RELOAD) {
      this.end();
      this.env.reload();
      return;
    }
    if (code === SYNC_CLOSE.BAD_FRAME) {
      this.reject('Соединение с сервером нарушено — обновите страницу');
    }
    this.emitStatus();
    this.scheduleReconnect();
    // Соединение не открылось: браузер не говорит почему. Сессия кончилась и доступ снят — не вечное «Нет сети».
    if (!this.ended) {
      void this.env.files.access().then((a) => {
        if (!this.ended && a && a.kind !== 'role') {
          this.end();
          this.env.access(a);
        }
      });
    }
  }

  private scheduleReconnect(): void {
    if (this.ended || this.reconnectTimer !== null) return;
    const base = Math.min(RECONNECT_MAX_MS, RECONNECT_MIN_MS * 2 ** this.attempt++);
    this.reconnectTimer = this.env.clock.setTimeout(
      () => {
        this.reconnectTimer = null;
        this.connect();
      },
      base * (0.5 + this.env.random() / 2),
    );
  }

  // ——— События ядра ———

  onCore(e: CoreSyncEvent): void {
    switch (e.type) {
      case 'syncSubscribe':
        this.pendingSubs++;
        this.emitStatus();
        this.send(encodeClientFrame({ t: 'subscribe', doc: e.doc, version: e.version }));
        break;
      case 'localImage':
        this.push({ kind: 'image', hash: e.hash, bytes: e.bytes });
        break;
      case 'localUpdates':
        // Соединение успело упасть: ядро соберёт их снова от версии сервера.
        if (this.ws && this.opened) this.push({ kind: 'update', doc: e.doc, clientSeq: e.clientSeq, bytes: e.bytes });
        break;
      case 'syncState':
        this.unacked = e.unacked;
        this.emitStatus();
        break;
      case 'imagesNeeded':
        for (const h of e.hashes) this.needImages.add(h);
        this.pumpImages();
        break;
    }
  }

  /** Ядро закончило `applyRemote`: следующая пачка. Пока оно занято, входящее складывается в одну. */
  onApplied(): void {
    this.applying = false;
    this.pumpIn();
  }

  private enqueueCmd(cmd: CoreCommand): void {
    this.inbound.push({ kind: 'cmd', cmd });
    this.pumpIn();
  }

  private enqueueApply(doc: string, bytes: Uint8Array): void {
    const last = this.inbound.at(-1);
    if (last?.kind === 'apply' && last.doc === doc) last.batches.push(bytes);
    else this.inbound.push({ kind: 'apply', doc, batches: [bytes] });
    this.pumpIn();
  }

  private pumpIn(): void {
    while (!this.applying && this.inbound.length > 0) {
      const item = this.inbound.shift()!;
      if (item.kind === 'cmd') {
        this.env.core.command(item.cmd);
      } else {
        this.applying = true;
        this.env.core.command({ type: 'applyRemote', doc: item.doc, batches: item.batches });
      }
    }
    this.emitStatus();
  }

  // ——— Исходящее: по порядку, картинка раньше правки со ссылкой на неё ———

  /** Подписки и правки отдаёт одна и та же команда ядра; правки ядро шлёт только после ответа на подписку. Очередь на отправку полна —
   * ядро ждёт, правки копятся в документе, а не в памяти этого worker'а. */
  private updateFlush(): void {
    const room = this.outBytes < MAX_OUT_BYTES;
    this.env.core.flush(!!this.ws && this.opened && !this.paused && !this.rejected && room);
  }

  private pumpOut(): void {
    if (this.sending) return;
    this.sending = true;
    void this.drainOut().finally(() => {
      this.sending = false;
      // Пока флаг ещё стоял, в очередь могло лечь новое: его никто бы не забрал.
      if (this.out.length > 0 && this.ws && this.opened && !this.rejected) this.pumpOut();
    });
  }

  private async drainOut(): Promise<void> {
    while (this.out.length > 0 && !this.ended) {
      const next = this.out[0];
      if (next.kind === 'image') {
        try {
          await this.env.files.uploadImage(next.hash, next.bytes);
          this.shiftOut();
        } catch (e) {
          const code = (e as { code?: string }).code;
          if (code === 'NETWORK' || code === 'UNAVAILABLE' || code === 'TOO_MANY_REQUESTS' || code === 'BUSY') {
            await this.sleep(IMAGE_RETRY_MS);
            continue;
          }
          this.reject('Картинка не загрузилась на сервер. Правки сохранены только в этом браузере — сохраните копию на диск');
          return;
        }
        continue;
      }
      if (!this.ws || !this.opened) return;
      // Медленный канал: `send` копил бы в браузере без предела.
      if (this.ws.bufferedAmount > SEND_HIGH_WATER) {
        await this.sleep(SEND_RETRY_MS);
        continue;
      }
      this.shiftOut();
      this.send(encodeClientFrame({ t: 'update', doc: next.doc, clientSeq: next.clientSeq, bytes: next.bytes }));
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      this.sendTimer = this.env.clock.setTimeout(() => {
        this.sendTimer = null;
        resolve();
      }, ms);
    });
  }

  // ——— Что запрашивается отдельно ———

  private async fetchSnapshot(doc: string, url: string): Promise<void> {
    // Снимок применён, а сервер снова просит снимок: дальше по кругу только трафик.
    const rounds = (this.snapshotRounds.get(doc) ?? 0) + 1;
    this.snapshotRounds.set(doc, rounds);
    if (rounds > MAX_SNAPSHOT_ROUNDS) {
      this.reject('Файл не удаётся догнать — обновите страницу. Правки сохранены в этом браузере');
      return;
    }
    this.snapshots++;
    this.env.core.flush(false);
    for (let wait = RESYNC_MIN_MS; !this.ended; wait = Math.min(RECONNECT_MAX_MS, wait * 2)) {
      try {
        const bytes = await this.env.files.snapshot(url);
        this.enqueueApply(doc, new Uint8Array(bytes));
        break;
      } catch (e) {
        const code = (e as { code?: string }).code;
        if (code === 'NOT_FOUND' || code === 'UNAUTHORIZED') {
          const a = await this.env.files.access();
          if (a && a.kind !== 'role') {
            this.end();
            this.env.access(a);
            return;
          }
        }
        await this.sleep(wait);
      }
    }
    if (--this.snapshots === 0 && this.ws && this.opened) this.resync();
  }

  private pumpImages(): void {
    for (const hash of this.needImages) {
      if (this.imagesActive >= IMAGE_CONCURRENCY) return;
      this.needImages.delete(hash);
      this.imagesActive++;
      void this.env.files
        .downloadImage(hash)
        .then((bytes) => {
          if (bytes) this.enqueueCmd({ type: 'addRemoteImage', hash, bytes: new Uint8Array(bytes) });
        })
        .catch(() => {
          // Сеть: ядро рисует заглушку, файл при следующем открытии получит картинку обычным путём.
        })
        .finally(() => {
          this.imagesActive--;
          this.pumpImages();
        });
    }
  }

  private emitStatus(detail?: string): void {
    const s = this.status();
    const key = `${s}|${detail ?? this.rejected ?? ''}`;
    if (key === this.lastStatus) return;
    this.lastStatus = key;
    this.env.status(s, detail ?? this.rejected ?? undefined);
  }

  private status(): SyncStatus {
    if (this.rejected) return 'rejected';
    if (this.paused) return 'paused';
    if (!this.ws || !this.opened) return this.lostOnce ? 'offline' : 'connecting';
    // «Синхронизировано» — когда все документы ответили и догоняющие правки уже в ядре. Обычная чужая правка индикатор не трогает:
    // она приходит, когда подписок в пути нет.
    if (this.readyDocs === 0 || this.pendingSubs > 0 || (this.catchingUp && (this.applying || this.inbound.length > 0)))
      return 'connecting';
    this.catchingUp = false;
    return this.unacked ? 'saving' : 'synced';
  }
}
