// Соединение файла со стороны клиента (спека подсистемы 4, разделы 7 и 8.1). Логика без DOM и без настоящего сокета: её запускает
// `worker/sync.worker.ts`, модульные тесты дают ей свой сокет с настоящим `bufferedAmount`. Основной поток трафика не видит:
// правки идут из worker'а ядра прямым `MessagePort`.
//
// Ничего не хранится здесь надолго. Что отправить, знает курсор ядра: на каждом новом соединении ядро подписывается своей версией,
// сервер отвечает своей, и ядро шлёт всё сверх неё. Поэтому обрыв, рестарт сервера, отказ «повторите» и пауза — один и тот же
// путь (`resync`), а неподтверждённое не теряется: оно в документе и в журнале браузера.
//
// Поколения. Каждый `resync` — новое поколение: ядро сбрасывает курсор (`syncReset`) и сообщает об этом (`onReset`), а всё, что
// оно прислало по порту до этого, — старое поколение и не уходит: ядро соберёт это заново после новой подписки. Отказы пачкам,
// ушедшим до `resync`, тоже старые: номера пачек у ядра растут, поэтому старые — не больше последнего номера до `resync`.

import {
  encodeClientFrame,
  encodePartBody,
  encodePresenceBody,
  parseServerFrame,
  PRESENCE_COLORS,
  snapshotPath,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  syncPath,
  type Affine6,
  type GestureKind,
  type PresenceItem,
  type Rect4,
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
  | { type: 'addRemoteImage'; hash: string; bytes: Uint8Array }
  /** Присутствие других участников (спека подсистемы 4, раздел 9): записи пачки сервера, уже разобранные. */
  | { type: 'presence'; items: PresenceItemForCore[] }
  /** Соединение заново: прежние участники уходят у ядра, своё состояние оно скажет снова. */
  | { type: 'presenceReset' };

/** Выделение и жест в кадре присутствия — те же поля, что у `PresenceSelection` и `PresenceGesture` протокола. */
export interface PresenceSelectionForCore {
  rev: number;
  count: number;
  parts: number;
  bounds: Rect4;
}
export interface PresenceGestureForCore {
  kind: GestureKind;
  transform: Affine6;
  frame: Affine6;
  k: number;
}

/** Запись присутствия для ядра (`PresenceItemDto`): цвет — `0xRRGGBB`, пустые поля — без значения. */
export type PresenceItemForCore =
  | { k: 'join'; participant: number; user: string; name: string; color: number }
  | {
      k: 'state';
      participant: number;
      page: string;
      view: Rect4;
      cursor?: [number, number];
      selection?: PresenceSelectionForCore;
      gesture?: PresenceGestureForCore;
    }
  | { k: 'part'; participant: number; rev: number; index: number; parts: number; ids: string[] }
  | { k: 'left'; participant: number };

/** Участник для шапки: номер соединения, человек, имя, цвет и страница. */
export interface Participant {
  id: number;
  user: string;
  name: string;
  color: string;
  page: string | null;
}

/** События ядра, которые идут этому классу. */
export type CoreSyncEvent =
  | { type: 'syncSubscribe'; doc: string; version: Uint8Array }
  | { type: 'localImage'; hash: string; bytes: Uint8Array }
  | { type: 'localUpdates'; doc: string; clientSeq: number; bytes: Uint8Array }
  | { type: 'syncState'; unacked: boolean }
  | { type: 'imagesNeeded'; hashes: string[] }
  /** Своё присутствие: не чаще 20 раз в секунду и при изменении; `parts` — узлы выделения новой версии. */
  | {
      type: 'presence';
      page: string;
      view: Rect4;
      cursor?: [number, number];
      selection?: PresenceSelectionForCore;
      parts?: Uint8Array[];
      gesture?: PresenceGestureForCore;
    };

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
  /** Участники файла для шапки: не чаще `PARTICIPANTS_MS` и только при изменении. */
  participants(list: Participant[]): void;
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
/** Отказ «повторите» (`UNAVAILABLE`, `GAP`): подписка заново через столько, с разбросом; роста паузы нет (бэклог). */
const RESYNC_MIN_MS = 1000;
/** Столько поколений подряд с `GAP` без единого `ack`: клиент не может послать правки в верном порядке. */
const MAX_GAPS = 6;
/** Столько применённых снимков подряд на документ без ответа `subscribed`. */
const MAX_SNAPSHOT_ROUNDS = 3;
/** Снимок не скачался столько раз по сети — соединение заново, обычным путём переподключения. */
const MAX_SNAPSHOT_ATTEMPTS = 4;
/** Сколько картинок качается разом. */
const IMAGE_CONCURRENCY = 4;
/** Журнал файла полон: ждём, пока сервер его сожмёт. */
const LOG_FULL_WAIT_MS = 30_000;
/** Кадры присутствия: не больше 25 в секунду (сервер принимает 30), лишнее сливается в последнее состояние. */
export const PRESENCE_RATE = 25;
export const PRESENCE_BURST = 40;
/** Повтор своего состояния, пока соединение живо: в скрытой вкладке кадров нет, а через 30 с тишины сервер скроет участника. */
export const PRESENCE_HEARTBEAT_MS = 10_000;
/** Список участников основному потоку не чаще. */
export const PARTICIPANTS_MS = 250;

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
  /** Срок на открытие сокета: зависшее рукопожатие браузер может держать минуты. */
  private openTimer: unknown = null;
  /** Ожидания отправки, картинок и снимков: их бывает несколько разом, остановка снимает все. */
  private readonly sleeps = new Set<unknown>();
  private opened = false;
  private ended = false;
  private rejected: string | null = null;
  private paused = false;
  private unacked = false;
  /** Подписки этого поколения: `false` — без ответа (файл ещё догоняет), `true` — сервер ответил. */
  private readonly subs = new Map<string, boolean>();
  /** Поколения подряд с `GAP` без `ack`; `gapCounted` — в этом поколении уже посчитан или был свой `UNAVAILABLE`: разрыв после
   * него — следствие отказа (пачки в пути опирались на отвергнутую), а не неверный порядок. */
  private gaps = 0;
  private gapCounted = false;
  /** Сбросы, которые ядро ещё не выполнило: всё, что оно шлёт до этого, — старое поколение. */
  private resets = 0;
  /** Наибольший номер пачки, взятой в отправку, и он же на момент последнего `resync`: отказы до него — прежнему поколению. */
  private lastSeq = -1;
  private oldSeq = -1;
  /** Ждём ответа о роли после `FORBIDDEN`: отправка выключена, вопрос один. */
  private roleCheck = false;
  private resyncs = 0;
  private readonly out: Out[] = [];
  private outBytes = 0;
  private sending = false;
  private readonly inbound: In[] = [];
  private applying = false;
  /** Документы, чей снимок качается: одна загрузка на документ, сколько бы раз сервер ни просил. */
  private readonly snapshotting = new Set<string>();
  /** Применённые снимки подряд на документ без ответа `subscribed`. */
  private readonly snapshotRounds = new Map<string, number>();
  private needImages = new Set<string>();
  private imagesActive = 0;
  private lastStatus: string | null = null;
  private generation = 0;
  /** Соединение было и пропало или не открылось: с этого момента — «нет сети», а не «подключается». */
  private lostOnce = false;
  /** С начала подписки до первого «всё на месте». */
  private catchingUp = true;
  /** Присутствие: роль подтверждена этим соединением (до этого сервер кадры отбрасывает), последнее своё состояние и части
   * выделения, ожидающие отправки кадры и бакет частоты. */
  private present = false;
  private ownState: Uint8Array | null = null;
  private ownParts: Uint8Array[] = [];
  private outState: Uint8Array | null = null;
  private outParts: Uint8Array[] = [];
  private tokens = PRESENCE_BURST;
  private tokensAt = 0;
  private presenceTimer: unknown = null;
  private heartbeat: unknown = null;
  private readonly participants = new Map<number, Participant>();
  private participantsTimer: unknown = null;
  private participantsDirty = false;

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
    for (const t of [
      this.reconnectTimer,
      this.pingTimer,
      this.pongTimer,
      this.pauseTimer,
      this.retryTimer,
      this.openTimer,
      this.presenceTimer,
      this.heartbeat,
      this.participantsTimer,
      ...this.sleeps,
    ])
      this.clock(t);
    this.reconnectTimer = this.pingTimer = this.pongTimer = this.pauseTimer = this.retryTimer = this.openTimer = null;
    this.presenceTimer = this.heartbeat = this.participantsTimer = null;
    this.sleeps.clear();
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
    this.clock(this.openTimer);
    this.openTimer = this.env.clock.setTimeout(() => {
      this.openTimer = null;
      // Не открылся за срок: как мёртвый сокет — закрыть и не ждать браузера, `online()` иначе его не заменит.
      if (generation === this.generation && !this.opened) this.dropSocket();
    }, PONG_WAIT_MS);
  }

  private onOpen(): void {
    this.clock(this.openTimer);
    this.openTimer = null;
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
    this.subs.clear();
    this.catchingUp = true;
    this.gapCounted = false;
    this.oldSeq = this.lastSeq;
    this.resets++;
    this.drop('update');
    this.enqueueCmd({ type: 'syncReset' });
    this.updateFlush();
    this.emitStatus();
  }

  /** Ядро выполнило `syncReset`: дальше по порту — новое поколение. */
  onReset(): void {
    this.resets = Math.max(0, this.resets - 1);
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
        // Роль подтверждена: сервер принимает присутствие. Своё — сразу, новое соединение о нём не знает.
        if (!this.present) {
          this.present = true;
          if (this.ownState) this.queuePresence(this.ownState, this.ownParts);
          this.armHeartbeat();
        }
        break;
      case 'presence':
        this.onPresence(f.items);
        break;
      case 'subscribed':
        // Ответ на подписку прежнего поколения (или без подписки): его версия ядру уже не нужна.
        if (this.subs.get(f.doc) !== false) break;
        this.subs.set(f.doc, true);
        this.attempt = 0;
        this.lostOnce = false;
        this.snapshotRounds.delete(f.doc);
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
        this.refused(f.code, f.clientSeq);
        break;
      case 'pause':
        this.pause(f.ms);
        break;
      case 'snapshot-needed':
        // Только для документа этого поколения; адрес — свой, из файла и документа, а не из кадра сети.
        if (!this.subs.has(f.doc)) break;
        this.subs.delete(f.doc);
        void this.fetchSnapshot(f.doc);
        this.emitStatus();
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

  private refused(code: SyncRefusal, clientSeq: number): void {
    // Пачка ушла до подписки заново: ядро её забыло и соберёт снова, отказ ей ничего не значит. Номер пачки — u32, за годы
    // работы одной вкладки по кругу не пройдёт.
    const stale = clientSeq <= this.oldSeq;
    switch (code) {
      case 'GAP':
        if (stale) return;
        // Пачки одного поколения в пути получают `GAP` одна за другой: это один разрыв.
        if (!this.gapCounted) {
          this.gapCounted = true;
          if (++this.gaps >= MAX_GAPS) {
            this.reject(
              'Сервер не принимает правки в этом порядке. Они сохранены только в этом браузере — сохраните копию на диск и обновите страницу',
            );
            return;
          }
        }
        this.resyncLater();
        break;
      case 'UNAVAILABLE':
        if (stale) return;
        this.gapCounted = true;
        this.resyncLater();
        break;
      case 'LOG_FULL':
        this.pause(LOG_FULL_WAIT_MS, 'Журнал файла на сервере переполнен. Правки ждут здесь, они не потеряются');
        break;
      case 'DOC_LIMIT':
        this.reject(
          'В файле слишком много страниц для сервера: правки новых страниц он не принимает. Они сохранены только в этом браузере — сохраните копию на диск',
        );
        break;
      case 'FORBIDDEN':
        // Роль сменилась: настоящее состояние скажет сервер, редактор перейдёт в просмотр и покажет, что правки не дошли. Пачки в
        // пути получают тот же отказ — вопрос один, и до ответа правки не уходят.
        if (this.roleCheck) return;
        this.roleCheck = true;
        this.updateFlush();
        void this.env.files.access().then((a) => {
          this.roleCheck = false;
          if (this.ended) return;
          if (a) this.env.access(a);
          this.updateFlush();
          // Право правки осталось (или ответа нет): курсор ядра уже за отвергнутыми пачками — без подписки заново они не уйдут,
          // пока следующая правка не получит `GAP`.
          if (!a || (a.kind === 'role' && a.role !== 'viewer')) this.resyncLater();
        });
        break;
      case 'BAD_UPDATE':
      case 'FOREIGN_PEER':
        this.reject('Сервер не принял правки. Они сохранены только в этом браузере — сохраните копию на диск и обновите страницу');
        break;
    }
  }

  /** Отказ, который повтор не исправит: правки дальше не уходят, чужие продолжают приходить, человек об этом знает. Шаг ядра не
   * выключается: он же отдаёт подписки — без него новые чужие страницы пусты, а после переподключения нет ни одной подписки.
   * Правки ядра отбрасываются здесь: отказ окончателен до перезагрузки, курсор ядра больше не нужен. */
  private reject(message: string): void {
    this.rejected = message;
    this.drop('update');
    this.updateFlush();
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
    this.clock(this.openTimer);
    this.openTimer = null;
    this.lostOnce = true;
    this.dropPresence();
    this.ws = null;
    this.opened = false;
    this.subs.clear();
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
        // Подписка прежнего поколения: после сброса ядро подпишется заново.
        if (this.resets > 0 || !this.ws || !this.opened) break;
        this.subs.set(e.doc, false);
        this.emitStatus();
        this.send(encodeClientFrame({ t: 'subscribe', doc: e.doc, version: e.version }));
        break;
      case 'localImage':
        // После отказа картинке незачем на сервер: правка со ссылкой на неё туда не уйдёт.
        if (this.rejected) break;
        this.push({ kind: 'image', hash: e.hash, bytes: e.bytes });
        break;
      case 'localUpdates':
        // Соединение успело упасть или пачка собрана до сброса: ядро соберёт её снова от версии сервера, а раньше новой подписки
        // она дала бы `GAP`. После отказа правки не уходят вовсе.
        if (this.rejected || this.resets > 0 || !this.ws || !this.opened) break;
        this.lastSeq = Math.max(this.lastSeq, e.clientSeq);
        this.push({ kind: 'update', doc: e.doc, clientSeq: e.clientSeq, bytes: e.bytes });
        break;
      case 'syncState':
        this.unacked = e.unacked;
        this.emitStatus();
        break;
      case 'imagesNeeded':
        for (const h of e.hashes) this.needImages.add(h);
        this.pumpImages();
        break;
      case 'presence':
        this.ownPresence(e);
        break;
    }
  }

  // ——— Присутствие (спека подсистемы 4, раздел 9) ———

  /** Своё состояние от ядра: кадр и, у новой версии выделения, её части. Без соединения — только запомнить. */
  private ownPresence(e: Extract<CoreSyncEvent, { type: 'presence' }>): void {
    let body: Uint8Array;
    try {
      body = encodePresenceBody({
        page: e.page,
        view: e.view,
        cursor: e.cursor ?? null,
        selection: e.selection ?? null,
        gesture: e.gesture ?? null,
      });
    } catch {
      // Ядро проверяет числа само; неверное сюда не доходит, а дошедшее не уходит в сеть.
      return;
    }
    this.ownState = body;
    const sel = e.selection;
    let parts: Uint8Array[] = [];
    if (e.parts && sel) {
      parts = e.parts.map((bytes, index) => encodePartBody({ rev: sel.rev, index, parts: e.parts!.length, bytes }));
      this.ownParts = parts;
    } else if (!sel) this.ownParts = [];
    // До роли очередь только копит последнее: уйдёт с ответом о роли.
    this.queuePresence(body, parts);
  }

  /** В очередь: состояние заменяет прежнее неотправленное, части новой версии — прежние. */
  private queuePresence(state: Uint8Array, parts: Uint8Array[]): void {
    this.outState = state;
    if (parts.length) this.outParts = [...parts];
    this.pumpPresence();
  }

  private pumpPresence(): void {
    if (!this.ws || !this.opened || !this.present) return;
    const now = this.env.clock.now();
    this.tokens = Math.min(PRESENCE_BURST, this.tokens + ((now - this.tokensAt) / 1000) * PRESENCE_RATE);
    this.tokensAt = now;
    while (this.tokens >= 1 && (this.outState || this.outParts.length)) {
      const body = this.outState ?? this.outParts.shift()!;
      const t = this.outState ? 'presence' : 'presence-part';
      this.outState = null;
      this.tokens -= 1;
      this.send(encodeClientFrame({ t, body }));
    }
    if ((this.outState || this.outParts.length) && this.presenceTimer === null) {
      this.presenceTimer = this.env.clock.setTimeout(() => {
        this.presenceTimer = null;
        this.pumpPresence();
      }, 1000 / PRESENCE_RATE);
    }
    this.armHeartbeat();
  }

  /** Повтор последнего состояния через `PRESENCE_HEARTBEAT_MS` после последнего кадра. */
  private armHeartbeat(): void {
    this.clock(this.heartbeat);
    this.heartbeat = this.env.clock.setTimeout(() => {
      this.heartbeat = null;
      if (this.present && this.ownState && !this.outState) this.queuePresence(this.ownState, []);
      else this.armHeartbeat();
    }, PRESENCE_HEARTBEAT_MS);
  }

  /** Соединения нет: участники пропадают, ядро забывает их; своё состояние уйдёт на следующем. */
  private dropPresence(): void {
    this.present = false;
    this.outState = null;
    this.outParts = [];
    this.clock(this.presenceTimer);
    this.clock(this.heartbeat);
    this.presenceTimer = this.heartbeat = null;
    this.env.core.command({ type: 'presenceReset' });
    if (this.participants.size > 0) {
      this.participants.clear();
      this.participantsChanged();
    }
  }

  /** Пачка сервера: ядру — записи, шапке — список участников. Порядок с правками не важен: ядро их не смешивает. */
  private onPresence(items: PresenceItem[]): void {
    const out: PresenceItemForCore[] = [];
    let changed = false;
    for (const it of items) {
      switch (it.k) {
        case 'join': {
          const color = PRESENCE_COLORS[it.color];
          const was = this.participants.get(it.participant);
          this.participants.set(it.participant, { id: it.participant, user: it.user, name: it.name, color, page: was?.page ?? null });
          changed = true;
          out.push({ k: 'join', participant: it.participant, user: it.user, name: it.name, color: parseInt(color.slice(1), 16) });
          break;
        }
        case 'state': {
          const s = it.state;
          const p = this.participants.get(it.participant);
          if (p && p.page !== s.page) {
            p.page = s.page;
            changed = true;
          }
          out.push({
            k: 'state',
            participant: it.participant,
            page: s.page,
            view: s.view,
            ...(s.cursor ? { cursor: s.cursor } : {}),
            ...(s.selection ? { selection: s.selection } : {}),
            ...(s.gesture ? { gesture: s.gesture } : {}),
          });
          break;
        }
        case 'part':
          out.push({ k: 'part', participant: it.participant, ...it.part });
          break;
        case 'left':
          changed = this.participants.delete(it.participant) || changed;
          out.push({ k: 'left', participant: it.participant });
          break;
      }
    }
    this.env.core.command({ type: 'presence', items: out });
    if (changed) this.participantsChanged();
  }

  /** Шапке — не чаще `PARTICIPANTS_MS`: присутствие приходит 30 раз в секунду, а список меняется редко. */
  private participantsChanged(): void {
    this.participantsDirty = true;
    if (this.participantsTimer !== null) return;
    this.env.participants([...this.participants.values()].map((p) => ({ ...p })));
    this.participantsDirty = false;
    this.participantsTimer = this.env.clock.setTimeout(() => {
      this.participantsTimer = null;
      if (this.participantsDirty) this.participantsChanged();
    }, PARTICIPANTS_MS);
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
    // Пока качается снимок, правки ждут: документ без подписки дал бы `GAP` на каждую.
    this.env.core.flush(!!this.ws && this.opened && !this.paused && !this.roleCheck && this.snapshotting.size === 0 && room);
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
      const t = this.env.clock.setTimeout(() => {
        this.sleeps.delete(t);
        resolve();
      }, ms);
      this.sleeps.add(t);
    });
  }

  // ——— Что запрашивается отдельно ———

  /** Снимок документа: одна загрузка, сколько бы раз сервер ни просил (каждое переподключение просит снова). Сеть не отдаёт его —
   * соединение заново обычным путём; сервер не отдаёт при живой роли — отказ с причиной, а не вечный повтор. */
  private async fetchSnapshot(doc: string): Promise<void> {
    if (this.snapshotting.has(doc)) return;
    // Снимок применён, а сервер снова просит снимок: дальше по кругу только трафик.
    if ((this.snapshotRounds.get(doc) ?? 0) >= MAX_SNAPSHOT_ROUNDS) {
      this.reject('Файл не удаётся догнать — обновите страницу. Правки сохранены в этом браузере');
      return;
    }
    this.snapshotting.add(doc);
    this.updateFlush();
    let bytes: ArrayBuffer | null = null;
    for (let attempt = 1, wait = RESYNC_MIN_MS; !this.ended && !this.rejected; attempt++, wait = Math.min(RECONNECT_MAX_MS, wait * 2)) {
      try {
        bytes = await this.env.files.snapshot(snapshotPath(this.env.file, doc));
        break;
      } catch (e) {
        const code = (e as { code?: string }).code;
        if (code === 'NOT_FOUND' || code === 'UNAUTHORIZED' || code === 'FORBIDDEN') {
          const a = await this.env.files.access();
          if (this.ended) break;
          if (a && a.kind !== 'role') {
            this.end();
            this.env.access(a);
            break;
          }
          if (a) {
            this.reject('Снимок файла не отдаётся сервером — обновите страницу. Правки сохранены в этом браузере');
            break;
          }
        }
        if (attempt >= MAX_SNAPSHOT_ATTEMPTS) {
          this.snapshotting.delete(doc);
          this.dropSocket();
          return;
        }
        await this.sleep(wait);
      }
    }
    this.snapshotting.delete(doc);
    if (bytes && !this.ended) {
      this.snapshotRounds.set(doc, (this.snapshotRounds.get(doc) ?? 0) + 1);
      this.enqueueApply(doc, new Uint8Array(bytes));
    }
    if (this.snapshotting.size === 0 && this.ws && this.opened && !this.ended) this.resync();
    else this.updateFlush();
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
    if (
      this.subs.size === 0 ||
      [...this.subs.values()].includes(false) ||
      this.snapshotting.size > 0 ||
      (this.catchingUp && (this.applying || this.inbound.length > 0))
    )
      return 'connecting';
    this.catchingUp = false;
    return this.unacked ? 'saving' : 'synced';
  }
}
