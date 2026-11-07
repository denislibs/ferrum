// WebSocket файлов (спека подсистемы 4, разделы 7.1–7.3): открытие с проверкой `Origin`, сессии и роли, кадры с
// пределами, порядок «проверка → журнал → ack и рассылка», догоняние с учётом медленного сокета. Сервер не падает ни
// от сбоя базы, ни от чужого клиента: всякий обработчик под `try`, очереди и память на соединение ограничены.

import {
  encodeServerFrame,
  encodeVersion,
  MAX_SYNC_FRAME_BYTES,
  readClientFrame,
  RANK_ROLE,
  ROLE_RANK,
  snapshotPath,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  versionIncludes,
  type CommentEvent,
  type FileRole,
  type RevokeReason,
  type ServerFrame,
  type SyncRefusal,
  type VersionMap,
} from '@ferrum/protocol';
import type { Pool } from 'pg';
import type { Auth } from '../auth';
import { authorizeUpgrade, type UpgradeSession } from '../auth-routes';
import type { BlobStore } from '../blob-store';
import { inTransaction } from '../db';
import { findDocSection } from './container-read';
import { PresenceHub, type PresenceLimits } from './presence';
import { UpdateLog, type Item, type LogLimits } from './update-log';
import { Validator } from './validator';

export interface ConnData {
  id: number;
  file: string;
  user: string;
  /** Сессия при открытии: выход на этом устройстве закрывает её соединения сразу. */
  session: string;
  /** Cookie открытия: периодическая проверка, что сессия жива (срок, снятие администратором). */
  cookie: string;
  role: FileRole;
  hello: boolean;
  docs: Map<string, DocSub>;
  frames: Bucket;
  ws: Bun.ServerWebSocket<ConnData> | null;
  closed: boolean;
  /** Кто ждёт освобождения сокета. */
  drained: (() => void)[];
  /** Сколько раз подряд прислал негодное. */
  strikes: number;
  helloTimer: Timer | null;
  lagSince: number;
  /** Имя человека для присутствия: из базы при `hello`, не из кадров клиента. */
  name: string;
  /** Роль подтверждена запросом `hello`: соединение получает события комментариев и считается «файл открыт» для писем. */
  confirmed: boolean;
  /** Пропущено событие комментариев (сокет не успевал): при освобождении — `gap`, клиент догонит по REST. */
  commentGap: boolean;
}

interface DocSub {
  /** Что есть у клиента: вектор подписки (только пиры, которые знает сервер) и всё, что ему отправлено после. По нему
   * видно, покрывает ли клиент снимок, когда сжатие убрало записи из-под догоняния. */
  version: VersionMap;
  /** Последняя запись, которую соединение уже получило. */
  sent: number;
  /** `true` — рассылка идёт напрямую; иначе соединение догоняет из базы. */
  live: boolean;
  pumping: boolean;
  /** Строка документа заведена: до этого правки не принимаются (журнал завёл бы её с пустой версией). */
  ready: boolean;
}

export interface SyncLimits {
  framesPerSecond: number;
  framesBurst: number;
  userFramesPerSecond: number;
  userFramesBurst: number;
  userBytesPerSecond: number;
  userBytesBurst: number;
  /** Открытий соединения на человека: переподключение не обнуляет пределы, а открытие стоит запроса к Valkey и базе. */
  userOpensPerSecond: number;
  userOpensBurst: number;
  /** Байты векторов подписки: свой бакет, маленький. Вектор держится в памяти, пока подписка ждёт базу, а настоящему
   * клиенту хватает килобайт. */
  userSubscribeBytesPerSecond: number;
  userSubscribeBytesBurst: number;
  /** Грубые нарушения, которых настоящий клиент не делает: пачка, роняющая Loro (стоит двух замен общего worker'а
   * проверки), и подделка чужого пира. Каждое закрывает соединение; годная пачка их не прощает. Бакет пуст — человек не
   * открывает соединений, пока он не наполнится. */
  userFaultsPerSecond: number;
  userFaultsBurst: number;
  /** Неудачных открытий с одного адреса (нет сессии, чужой `Origin`, вкладка за другого): каждое стоит запроса к Valkey,
   * а пределы человека до входа не работают. */
  ipFailedOpensPerSecond: number;
  ipFailedOpensBurst: number;
  connectionsPerUser: number;
  connectionsTotal: number;
  /** Строк документов, которые человек заводит разом: каждая читает снимок документа целиком. Остальные подписки ждут. */
  userDocStatesAtOnce: number;
  subscribesPerConnection: number;
  /** Буфер сокета: выше — рассылка этому соединению идёт с паузой и из базы. */
  highWaterBytes: number;
  /** Не вычитывает дольше — закрывается. */
  stalledMs: number;
  /** Плохих кадров подряд, дальше соединение закрывается. */
  strikes: number;
  helloMs: number;
  recheckMs: number;
  sessionCheckMs: number;
  /** Новейшая схема документа, которую знает эта сборка: `hello` выше не поднимет `file.schema_version`. */
  knownSchema: number;
  /** Записей журнала за один запрос догоняния. */
  pumpRows: number;
  log: Partial<LogLimits>;
  presence: Partial<PresenceLimits>;
  /** Буфер сокета, выше которого событие комментариев пропускается (потом — `gap`): очереди на соединение нет. */
  commentHighWaterBytes: number;
}

/** Схема документа этой сборки: `SCHEMA_VERSION` ядра (`crates/ferrum-doc/src/document.rs`), сервер и клиент выходят
 * одной сборкой. Тест сверяет их. */
export const SERVER_SCHEMA = 1;

/** Настоящая нагрузка и запас (`hub.test.ts` прогоняет перетаскивание, набор и большую вставку через эти пределы):
 * ядро шлёт пачку на документ не чаще 30 раз в секунду, поэтому 120 кадров в секунду с запасом хватает и на правки
 * двух документов разом, и на подписки при открытии файла со многими страницами. */
export const SYNC_LIMITS: SyncLimits = {
  framesPerSecond: 120,
  framesBurst: 2000,
  userFramesPerSecond: 480,
  userFramesBurst: 4000,
  userBytesPerSecond: 64 * 1024 * 1024,
  userBytesBurst: 256 * 1024 * 1024,
  userOpensPerSecond: 1,
  userOpensBurst: 64,
  userSubscribeBytesPerSecond: 4 * 1024 * 1024,
  userSubscribeBytesBurst: 16 * 1024 * 1024,
  userFaultsPerSecond: 1 / 300,
  userFaultsBurst: 3,
  ipFailedOpensPerSecond: 1,
  ipFailedOpensBurst: 64,
  connectionsPerUser: 64,
  connectionsTotal: 20_000,
  userDocStatesAtOnce: 2,
  subscribesPerConnection: 4096,
  highWaterBytes: 1024 * 1024,
  stalledMs: 120_000,
  strikes: 16,
  helloMs: 15_000,
  recheckMs: 30_000,
  sessionCheckMs: 60_000,
  knownSchema: SERVER_SCHEMA,
  pumpRows: 512,
  log: {},
  presence: {},
  commentHighWaterBytes: 64 * 1024,
};

/** Сколько событие комментариев ждёт соседа с меньшим номером, и сколько таких ждёт разом на файл. */
const COMMENT_REORDER_MS = 50;
const COMMENT_EARLY_MAX = 256;
/** Событий в очереди рассылки файла, пока идёт проход: дальше — `gap` всем. */
const COMMENT_QUEUE_MAX = 1024;
/** Столько байт записей журнала за один запрос догоняния. */
const PUMP_BYTES = 8 * 1024 * 1024;
/** Буфер сокета, сверх которого Bun кадр выбрасывает. Кадр шлётся, только пока буфер не выше `highWaterBytes`, поэтому
 * предел — это порог, самый большой кадр и запас на служебные кадры: в работе он не достигается, а если достигнут —
 * соединение закрывается и догонит заново, а не теряет кадр молча. */
const BACKPRESSURE_LIMIT = 2 * 1024 * 1024 + MAX_SYNC_FRAME_BYTES + 8 * 1024 * 1024;

class Bucket {
  private tokens: number;
  private last = performance.now();
  constructor(
    private readonly rate: number,
    private readonly burst: number,
  ) {
    this.tokens = burst;
  }
  /** Сколько миллисекунд ждать `n`, ничего не беря. */
  peek(n: number): number {
    this.refill();
    return this.tokens >= n ? 0 : Math.ceil(((n - this.tokens) / this.rate) * 1000);
  }
  /** Взять `n`; не хватает — сколько миллисекунд ждать. */
  take(n: number): number {
    this.refill();
    if (this.tokens >= n) {
      this.tokens -= n;
      return 0;
    }
    return Math.ceil(((n - this.tokens) / this.rate) * 1000);
  }
  /** Полон: забыть его — то же, что завести заново. */
  full(): boolean {
    this.refill();
    return this.tokens >= this.burst;
  }
  private refill(): void {
    const now = performance.now();
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
  }
}

/** Пределы человека живут дольше его соединений: уходят, только когда все бакеты снова полны. */
interface UserState {
  frames: Bucket;
  bytes: Bucket;
  opens: Bucket;
  subscribeBytes: Bucket;
  faults: Bucket;
  conns: number;
  /** Строки документов, которые заводятся сейчас, и подписки, ждущие места. */
  creating: number;
  creatingQueue: (() => void)[];
}

/** Строка документа: версия сервера и версия снимка. */
interface DocRowState {
  vv: Record<string, number>;
  base_vv: Record<string, number>;
}

const toMap = (o: Record<string, number>): VersionMap => new Map(Object.entries(o));

export class SyncHub {
  readonly validator: Validator;
  readonly log: UpdateLog;
  /** Присутствие участников (спека, раздел 9): своё у каждого файла, по соединениям с подтверждённой ролью. */
  readonly presence: PresenceHub;
  private readonly limits: SyncLimits;
  private readonly conns = new Map<string, Set<ConnData>>();
  private readonly users = new Map<string, UserState>();
  /** Бакеты неудачных открытий по адресам: уходят, когда снова полны. */
  private readonly failedOpens = new Map<string, Bucket>();
  private total = 0;
  private seq = 0;
  private readonly timers: Timer[] = [];
  private stopped = false;
  /** Идущие обработчики кадров и догоняния: остановка ждёт их, прежде чем отдать пул. */
  private readonly inflight = new Set<Promise<unknown>>();
  /** Строки документов, которые заводятся сейчас: подписки на тот же документ ждут ту же, а не читают снимок снова. */
  private readonly creating = new Map<string, Promise<DocRowState | null | 'busy'>>();
  /** Идущая сверка ролей всех файлов и нужна ли ещё одна после неё. */
  private rechecking: Promise<void> | null = null;
  private recheckAgain = false;
  /** Комментарии (раздел 10.4): очередь рассылки каждого файла (рассылает один проход за раз, всё накопленное — одним
   * запросом ролей) и последний разосланный номер файла (событие с номером не больше уже не нужно: клиент, получивший
   * больший, догоняет по REST). */
  private readonly commentRooms = new Map<string, { queue: CommentEvent[]; busy: boolean; overflow: boolean }>();
  private readonly commentHeads = new Map<string, number>();
  /** Перестановка: последний поставленный в цепочку номер файла и пришедшие раньше своей очереди. Ответы REST о соседних
   * изменениях приходят сюда не по порядку фиксации; подождать соседа дешевле, чем каждому клиенту догонять по REST. */
  private readonly commentQueued = new Map<string, number>();
  private readonly commentEarly = new Map<string, { evs: CommentEvent[]; timer: Timer }>();
  /** Пропущено событий комментариев медленным соединениям: тесты и проба. */
  commentSkipped = 0;
  /** Замер §13.1: от прихода кадра до конца его рассылки, мс. */
  onTiming: ((ms: number) => void) | null = null;
  /** Вызывается после фиксации: сжатие считает записи журнала; при уходе последнего — тоже. */
  onLog: (file: string, doc: string, logCount: number, logBytes: number) => void = () => {};
  onLastLeft: (file: string) => void = () => {};

  constructor(
    private readonly pool: Pool,
    private readonly blobs: BlobStore,
    private readonly auth: Auth,
    private readonly publicUrl: string,
    limits: Partial<SyncLimits> = {},
  ) {
    this.limits = { ...SYNC_LIMITS, ...limits };
    this.validator = new Validator(pool, blobs);
    this.log = new UpdateLog(pool, this.limits.log);
    this.presence = new PresenceHub(this.limits.presence);
    this.log.onCommitted = (items, states) => {
      for (const { item, seq } of items) this.broadcast(item, seq);
      for (const s of states) this.onLog(s.file, s.doc, s.logCount, s.logBytes);
    };
    this.timers.push(setInterval(() => void this.recheckAll(), this.limits.recheckMs));
    this.timers.push(setInterval(() => void this.checkSessions().catch(report), this.limits.sessionCheckMs));
    this.timers.push(
      setInterval(
        () => {
          this.closeStalled();
          this.sweepUsers();
        },
        Math.min(5000, this.limits.stalledMs),
      ),
    );
    for (const t of this.timers) t.unref?.();
  }

  /** Проверка перед открытием: `Origin` и живая сессия (`authorizeUpgrade`), вкладка за того же человека, бан, предел
   * открытий и соединений, роль. Всё, что не стоит запроса, — раньше запросов: отказанный не тратит ни Valkey, ни базу.
   * `ip` — адрес клиента для предела неудачных открытий. */
  async upgrade(req: Request, server: Bun.Server<ConnData>, file: string, ip = 'unknown'): Promise<Response | undefined> {
    if (this.stopped) return new Response(null, { status: 503 });
    const failed = this.failedOpens.get(ip);
    const cooling = failed?.peek(1) ?? 0;
    if (cooling > 0) return tooMany(cooling);
    let session: UpgradeSession | null;
    try {
      session = await authorizeUpgrade(req, this.auth, this.publicUrl);
    } catch {
      return new Response(null, { status: 503 });
    }
    // Вкладка работает за другого человека (вход сменили в другой вкладке): правки X не пишутся под Y.
    if (!session || new URL(req.url).searchParams.get('user') !== session.userId) {
      this.failedOpen(ip);
      return new Response(null, { status: 401 });
    }
    const u = this.user(session.userId);
    // Грубые нарушения исчерпали бакет: до его пополнения соединений нет.
    const banned = u.faults.peek(1);
    if (banned > 0) return tooMany(banned);
    if (this.total >= this.limits.connectionsTotal || u.conns >= this.limits.connectionsPerUser) return tooMany(5000);
    const wait = u.opens.take(1);
    if (wait > 0) return tooMany(wait);
    let role: FileRole | null;
    try {
      role = await this.role(file, session.userId);
    } catch {
      return new Response(null, { status: 503 });
    }
    if (!role) return new Response(null, { status: 404 });
    const data: ConnData = {
      id: ++this.seq,
      file,
      user: session.userId,
      session: session.sessionId,
      cookie: req.headers.get('cookie') ?? '',
      role,
      hello: false,
      docs: new Map(),
      frames: new Bucket(this.limits.framesPerSecond, this.limits.framesBurst),
      ws: null,
      closed: false,
      drained: [],
      strikes: 0,
      helloTimer: null,
      lagSince: 0,
      name: '',
      confirmed: false,
      commentGap: false,
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
    return undefined;
  }

  private failedOpen(ip: string): void {
    let b = this.failedOpens.get(ip);
    if (!b) this.failedOpens.set(ip, (b = new Bucket(this.limits.ipFailedOpensPerSecond, this.limits.ipFailedOpensBurst)));
    b.take(1);
  }

  private user(id: string): UserState {
    let u = this.users.get(id);
    if (!u) {
      u = {
        frames: new Bucket(this.limits.userFramesPerSecond, this.limits.userFramesBurst),
        bytes: new Bucket(this.limits.userBytesPerSecond, this.limits.userBytesBurst),
        opens: new Bucket(this.limits.userOpensPerSecond, this.limits.userOpensBurst),
        subscribeBytes: new Bucket(this.limits.userSubscribeBytesPerSecond, this.limits.userSubscribeBytesBurst),
        faults: new Bucket(this.limits.userFaultsPerSecond, this.limits.userFaultsBurst),
        conns: 0,
        creating: 0,
        creatingQueue: [],
      };
      this.users.set(id, u);
    }
    return u;
  }

  /** Человек без соединений с полными бакетами ничем не отличается от нового. */
  private sweepUsers(): void {
    for (const [id, u] of this.users) {
      if (
        u.conns === 0 &&
        u.creating === 0 &&
        u.frames.full() &&
        u.bytes.full() &&
        u.opens.full() &&
        u.subscribeBytes.full() &&
        u.faults.full()
      )
        this.users.delete(id);
    }
    for (const [ip, b] of this.failedOpens) if (b.full()) this.failedOpens.delete(ip);
  }

  private async role(file: string, user: string): Promise<FileRole | null> {
    const { rows } = await this.pool.query<{ rank: number | null }>('SELECT file_role_rank($1, $2) AS rank', [file, user]);
    return RANK_ROLE[rows[0]?.rank ?? 0];
  }

  /** Настройка `websocket` для `Bun.serve`. */
  readonly websocket: Bun.WebSocketHandler<ConnData> = {
    maxPayloadLength: 16 * 1024 * 1024 + 1024,
    idleTimeout: 120,
    sendPings: true,
    // Память на медленного читателя: выше `highWaterBytes` рассылка ему идёт из базы, кадр не теряется.
    backpressureLimit: BACKPRESSURE_LIMIT,
    closeOnBackpressureLimit: true,
    open: (ws) => {
      guard(() => this.opened(ws));
    },
    message: (ws, message) => {
      guard(() => this.received(ws, message));
    },
    drain: (ws) => {
      guard(() => {
        ws.data.lagSince = 0;
        for (const w of ws.data.drained.splice(0)) w();
        this.commentDrained(ws.data);
      });
    },
    close: (ws) => {
      guard(() => this.closed(ws));
    },
  };

  private opened(ws: Bun.ServerWebSocket<ConnData>): void {
    const d = ws.data;
    d.ws = ws;
    this.total++;
    let set = this.conns.get(d.file);
    if (!set) this.conns.set(d.file, (set = new Set()));
    set.add(d);
    this.user(d.user).conns++;
    d.helloTimer = setTimeout(() => {
      if (!d.hello) ws.close(SYNC_CLOSE.BAD_FRAME, 'hello');
    }, this.limits.helloMs);
  }

  private closed(ws: Bun.ServerWebSocket<ConnData>): void {
    const d = ws.data;
    if (d.closed) return;
    d.closed = true;
    d.ws = null;
    if (d.helloTimer) clearTimeout(d.helloTimer);
    for (const w of d.drained.splice(0)) w();
    this.total--;
    const u = this.users.get(d.user);
    if (u) u.conns--;
    // Ушёл — пропадает у других сразу, а не через 30 с (спека, раздел 9).
    this.presence.leave(d);
    const set = this.conns.get(d.file);
    set?.delete(d);
    if (set?.size === 0) {
      this.conns.delete(d.file);
      this.commentHeads.delete(d.file);
      this.commentQueued.delete(d.file);
      this.onLastLeft(d.file);
    }
  }

  /** Служебный кадр. Сокет его выбросил — соединение закрывается: клиент переподключится и узнает всё заново. */
  private send(d: ConnData, frame: ServerFrame): boolean {
    return this.sendBytes(d, encodeServerFrame(frame));
  }

  /** `false` — кадр не ушёл: соединение закрыто или закрывается. */
  private sendBytes(d: ConnData, bytes: Uint8Array): boolean {
    if (d.closed || !d.ws) return false;
    if (d.ws.sendBinary(bytes) !== 0) return true;
    d.ws.close(SYNC_CLOSE.SLOW, 'slow');
    return false;
  }

  /** Обработчик в учёте: остановка дождётся его. */
  private track(p: Promise<void>): void {
    this.inflight.add(p);
    void p.finally(() => this.inflight.delete(p));
  }

  private strike(d: ConnData): void {
    if (++d.strikes >= this.limits.strikes) d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'bad frames');
  }

  /** Грубое нарушение: соединение закрывается сразу, нарушение копится на человеке. Годная пачка его не прощает: иначе
   * чередование с ней роняло бы общий worker проверки без конца. Бакет исчерпан — закрываются все соединения человека. */
  private fault(d: ConnData, reason: string): void {
    const u = this.user(d.user);
    u.faults.take(1);
    if (u.faults.peek(1) > 0) {
      for (const set of this.conns.values()) for (const x of set) if (x.user === d.user) x.ws?.close(SYNC_CLOSE.BAD_FRAME, reason);
    }
    d.ws?.close(SYNC_CLOSE.BAD_FRAME, reason);
  }

  private received(ws: Bun.ServerWebSocket<ConnData>, message: string | Buffer): void {
    const d = ws.data;
    if (typeof message === 'string') {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'text');
      return;
    }
    const at = performance.now();
    const frame = readClientFrame(message);
    if (!frame) {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'frame');
      return;
    }
    if (frame.t === 'ping') {
      this.send(d, { t: 'pong' });
      return;
    }
    if (!d.hello && frame.t !== 'hello') {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'hello first');
      return;
    }
    if (frame.t === 'hello') {
      // Один раз на соединение, а открытия ограничены: пауза на `hello` оставила бы соединение без него навсегда.
      this.hello(d, frame.protocol, frame.schema);
      return;
    }
    // Присутствие — мимо бакетов правок: его пределы свои, лишнее молча отбрасывается, а пауза правок от курсора была бы
    // ложной. Кадр подписывает соединение: кто, имя и цвет — с сервера. Тело разбирает `PresenceHub` после своих пределов.
    if (frame.t === 'presence') {
      this.presence.state(d, d.name, frame.body);
      return;
    }
    if (frame.t === 'presence-part') {
      this.presence.part(d, frame.body);
      return;
    }
    const u = this.user(d.user);
    // Байты правки — в общий бакет; вектор подписки — в свой, маленький: он живёт в памяти, пока подписка ждёт базу.
    const size = frame.t === 'update' ? frame.bytes.length : 0;
    const vector = frame.t === 'subscribe' ? frame.version.length : 0;
    // Все бакеты сразу: неудачный кадр не тратит токены одного, пока ждёт другой.
    const wait = Math.max(d.frames.peek(1), u.frames.peek(1), u.bytes.peek(size), u.subscribeBytes.peek(vector));
    if (wait > 0) {
      // Предел частоты — пауза: кадр не принят, клиент пришлёт его снова. Подписка и правка равны перед ним.
      this.send(d, { t: 'pause', ms: Math.min(wait, 30_000) });
      return;
    }
    d.frames.take(1);
    u.frames.take(1);
    u.bytes.take(size);
    u.subscribeBytes.take(vector);
    switch (frame.t) {
      case 'subscribe':
        this.track(
          this.subscribe(d, frame.doc, frame.version).catch((e: unknown) => {
            if (d.closed) return;
            console.error('Подписка:', e instanceof Error ? e.message : e);
            // База или проверка недоступна: соединение закрывается, клиент переподключится и подпишется заново.
            d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
          }),
        );
        break;
      case 'update':
        this.track(
          this.update(d, frame.doc, frame.clientSeq, frame.bytes, at).catch((e: unknown) => {
            console.error('Приём пачки:', e instanceof Error ? e.message : e);
            this.refuse(d, frame.doc, frame.clientSeq, 'UNAVAILABLE');
          }),
        );
        break;
    }
  }

  private hello(d: ConnData, protocol: number, schema: number): void {
    if (d.hello) return;
    if (protocol !== SYNC_PROTOCOL) {
      this.send(d, { t: 'please-reload' });
      d.ws?.close(SYNC_CLOSE.RELOAD, 'protocol');
      return;
    }
    d.hello = true;
    if (d.helloTimer) clearTimeout(d.helloTimer);
    d.helloTimer = null;
    // Схему поднимает только тот, кто может писать, и не выше известной этой сборке; без изменения строка не трогается.
    this.track(
      this.pool
        .query<{ version: string; member: number; rank: number | null; name: string | null }>(
          `WITH f AS (SELECT id, version, file_role_rank(id, $2) AS rank, file_member_rank(id, $2) AS member FROM file WHERE id = $1),
                up AS (UPDATE file SET schema_version = $3 FROM f
                       WHERE file.id = f.id AND f.rank >= $4 AND file.schema_version < $3 RETURNING 1)
           SELECT version, member, rank,
                  (SELECT name FROM "user" WHERE id = $2) AS name
           FROM f`,
          [d.file, d.user, Math.min(schema, this.limits.knownSchema), ROLE_RANK.editor],
        )
        .then(({ rows }) => {
          // Роль — по этому запросу, а не по открытию: сверка ролей пропускает соединение, открытое пока она шла, и роль,
          // прочитанная до снятия, иначе жила бы до следующей сверки.
          const role = rows[0] ? RANK_ROLE[rows[0].rank ?? 0] : null;
          if (!role) {
            this.revoke(d, 'access');
            return;
          }
          d.role = role;
          d.name = rows[0].name ?? '';
          this.send(d, { t: 'role', role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
          // Присутствие и комментарии файла — только после роли, подтверждённой этим запросом: снятый доступ их не увидит.
          d.confirmed = true;
          this.presence.admit(d);
        })
        .catch((e: unknown) => {
          if (d.closed) return;
          report(e);
          // Без `role` клиент не начнёт работу: пусть переподключится.
          d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
        }),
    );
  }

  private refuse(d: ConnData, doc: string, clientSeq: number, code: SyncRefusal): void {
    this.send(d, { t: 'refused', doc, clientSeq, code });
  }

  private async update(d: ConnData, doc: string, clientSeq: number, frame: Uint8Array, at: number): Promise<void> {
    // Буфер кадра живёт только на время обработчика сокета: дальше — своя копия.
    const bytes = frame.slice();
    // Читатель не пишет: и по последней роли соединения, и в транзакции записи. Не удар: после понижения пачки в пути
    // приходят от настоящего клиента.
    if (ROLE_RANK[d.role] < ROLE_RANK.editor) {
      this.refuse(d, doc, clientSeq, 'FORBIDDEN');
      return;
    }
    const sub = d.docs.get(doc);
    // Подписки нет (её сняло `snapshot-needed`, пачки уже были в пути) — клиент подпишется заново.
    if (!sub) {
      this.refuse(d, doc, clientSeq, 'GAP');
      return;
    }
    // Строка документа ещё не заведена: журнал завёл бы её с пустой версией вместо версии снимка.
    if (!sub.ready) {
      this.refuse(d, doc, clientSeq, 'UNAVAILABLE');
      return;
    }
    const verdict = await this.validator.validate(d.file, doc, bytes, d.user);
    if (verdict.kind === 'bad') {
      this.refuse(d, doc, clientSeq, 'BAD_UPDATE');
      // Пачка уронила Loro дважды, на свежем worker'е тоже: это не ошибка разбора, а удар по всему серверу.
      if (verdict.fatal) this.fault(d, 'panic');
      else this.strike(d);
      return;
    }
    if (verdict.kind === 'retry') {
      this.refuse(d, doc, clientSeq, 'UNAVAILABLE');
      return;
    }
    if (verdict.pending) {
      this.refuse(d, doc, clientSeq, 'GAP');
      return;
    }
    d.strikes = 0;
    const item: Item = { file: d.file, doc, user: d.user, bytes, start: verdict.start, end: verdict.end, chain: verdict.chain };
    (item as ItemFrom).from = d;
    (item as ItemFrom).at = at;
    const outcome = await this.log.submit(item);
    if (outcome.kind === 'refused') {
      // Документ проверки уже принял пачку, а журнал нет: он больше не равен журналу.
      this.validator.reject(verdict.chain);
      this.refuse(d, doc, clientSeq, outcome.code);
      // Чужой пир настоящий клиент не шлёт, а подделка отравляет цепочку проверки соавторам: их пачки получают `GAP`. Ничей
      // пир снимка — не удар: его может прислать и честный клиент (правки прошлого открытия).
      if (outcome.fault) this.fault(d, 'foreign peer');
      return;
    }
    this.send(d, { t: 'ack', doc, clientSeq, seq: outcome.seq });
  }

  /** Правка уходит подписчикам этого документа в порядке `seq`; отправителю только `ack`. */
  private broadcast(item: Item, seq: number): void {
    const set = this.conns.get(item.file);
    if (!set) return;
    const from = (item as ItemFrom).from;
    let frame: Uint8Array | null = null;
    for (const d of set) {
      const sub = d.docs.get(item.doc);
      if (!sub) continue;
      if (d === from) {
        // Своя правка: соединению она не нужна, а номер двигается, иначе догоняние пошлёт её обратно.
        if (sub.live && sub.sent === seq - 1) sub.sent = seq;
        merge(sub.version, item.end);
        continue;
      }
      if (!sub.live) continue;
      // Пропуск в рассылке (соединение отстало) — только через догоняние из базы.
      if (sub.sent !== seq - 1 || this.slow(d)) {
        this.goLag(d, item.doc, sub);
        continue;
      }
      frame ??= encodeServerFrame({ t: 'updates', doc: item.doc, seq, bytes: item.bytes });
      if (!this.sendBytes(d, frame)) continue;
      sub.sent = seq;
      merge(sub.version, item.end);
    }
    const at = (item as ItemFrom).at;
    if (at !== undefined) this.onTiming?.(performance.now() - at);
  }

  private slow(d: ConnData): boolean {
    return !!d.ws && d.ws.getBufferedAmount() > this.limits.highWaterBytes;
  }

  private goLag(d: ConnData, doc: string, sub: DocSub): void {
    sub.live = false;
    if (!d.lagSince) d.lagSince = performance.now();
    if (!sub.pumping) this.track(this.pump(d, doc, sub, false).catch((e: unknown) => this.pumpFailed(d, e)));
  }

  private pumpFailed(d: ConnData, e: unknown): void {
    if (d.closed) return;
    console.error('Догоняние:', e instanceof Error ? e.message : e);
    d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
  }

  /** Сокет не выше порога. `drain` будит всех ждущих разом, а кадр одного снова заполняет буфер: проверка — после каждого. */
  private async whenWritable(d: ConnData): Promise<void> {
    while (!d.closed && this.slow(d)) {
      if (!d.lagSince) d.lagSince = performance.now();
      await new Promise<void>((resolve) => d.drained.push(resolve));
    }
  }

  private async subscribe(d: ConnData, doc: string, frame: Uint8Array): Promise<void> {
    // Вектор не разбирается в `Map` до ответа базы: разобранный он в восемь раз больше кадра и жил бы через `await`. Байты
    // кадра — своя копия: буфер сокета живёт только на время обработчика. Разбор уже проверен `readClientFrame`.
    const raw = frame.slice();
    const old = d.docs.get(doc);
    if (!old && d.docs.size >= this.limits.subscribesPerConnection) {
      d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'too many docs');
      return;
    }
    const sub: DocSub = { version: new Map(), sent: 0, live: false, pumping: false, ready: false };
    d.docs.set(doc, sub);
    const state = await this.docState(d, doc);
    if (d.closed || d.docs.get(doc) !== sub) return;
    // Очередь проверки полна снимками: подписка не принята, клиент повторит после паузы.
    if (state === 'busy') {
      d.docs.delete(doc);
      this.send(d, { t: 'pause', ms: 1000 });
      return;
    }
    // Клиент отстал сильнее журнала: снимок вобрал правки, которых у него нет.
    if (state && Object.entries(state.base_vv).some(([peer, c]) => counterOf(raw, peer) < c)) {
      this.needSnapshot(d, doc);
      return;
    }
    // Память на подписку — по пирам документа на сервере, а не по вектору клиента: пиры, которых сервер не знает, ни
    // одну запись не покрывают.
    for (const peer of Object.keys(state?.vv ?? {})) {
      const c = counterOf(raw, peer);
      if (c > 0) sub.version.set(peer, c);
    }
    sub.ready = true;
    await this.pump(d, doc, sub, true);
  }

  /** Строка документа. Нет её — заводится с версией снимка файла: одна на документ, сколько бы подписок её ни ждали, и не
   * больше `userDocStatesAtOnce` разом у человека. Заводит и читатель: версия снимка верна для всех, а строк не больше, чем
   * документов в контейнере. У документа без снимка (страница, в которую ещё не писали) строки нет, её заведёт первая запись. */
  private async docState(d: ConnData, doc: string): Promise<DocRowState | null | 'busy'> {
    const found = await this.pool.query<DocRowState>('SELECT vv, base_vv FROM doc_state WHERE file_id = $1 AND doc = $2', [d.file, doc]);
    if (found.rows[0]) return found.rows[0];
    const key = `${d.file}/${doc}`;
    let going = this.creating.get(key);
    if (!going) {
      const made = this.createDocState(this.user(d.user), d.file, doc).finally(() => {
        if (this.creating.get(key) === made) this.creating.delete(key);
      });
      this.creating.set(key, made);
      going = made;
    }
    return going;
  }

  private async createDocState(u: UserState, file: string, doc: string): Promise<DocRowState | null | 'busy'> {
    if (u.creating < this.limits.userDocStatesAtOnce) u.creating++;
    // Место передаётся из рук в руки: счётчик не меняется.
    else await new Promise<void>((resolve) => u.creatingQueue.push(resolve));
    try {
      // Файл записали целиком, пока шло чтение: снимок прежний, строка с его версией была бы неверной.
      for (let attempt = 0; ; attempt++) {
        const { rows } = await this.pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
        const contentKey = rows[0]?.content_key ?? null;
        const section = await findDocSection(this.blobs, contentKey, doc);
        if (!section) return null;
        const base = await this.validator.baseOf(section);
        if (base === 'busy') return 'busy';
        // Сбой проверки — не пустая версия: строка с пустой версией дала бы `GAP` на каждую правку. Подписка повторится.
        if (!base) throw new Error('проверка недоступна');
        const made = await inTransaction(this.pool, async (c) => {
          // Порядок блокировок общий: строка файла, потом строки документов. Запись файла целиком (`FOR UPDATE`) ждёт этой
          // транзакции, а прошедшая видна по `content_key`.
          const f = await c.query<{ content_key: string | null; content_by: string | null }>(
            'SELECT content_key, content_by FROM file WHERE id = $1 FOR KEY SHARE',
            [file],
          );
          if ((f.rows[0]?.content_key ?? null) !== contentKey) return 'changed' as const;
          const put = await c.query(
            `INSERT INTO doc_state (file_id, doc, vv, base_vv) VALUES ($1, $2, $3::jsonb, $3::jsonb) ON CONFLICT DO NOTHING`,
            [file, doc, JSON.stringify(base)],
          );
          if (put.rowCount === 0) {
            const had = await c.query<DocRowState>('SELECT vv, base_vv FROM doc_state WHERE file_id = $1 AND doc = $2', [file, doc]);
            return had.rows[0] ?? ('changed' as const);
          }
          // Пиры снимка — за тем, кто записал содержимое целиком: он и продолжает их (первая запись нового файла идёт тем же
          // пиром, что и правки за ней). Иначе первый пишущий занял бы пир автора — подделка его истории. Автор неизвестен —
          // пир ничей, продолжать его не может никто. Уже занятые журналом остаются за своими людьми.
          await c.query(
            `INSERT INTO doc_peer (file_id, peer, user_id) SELECT $1::uuid, p::numeric, $3 FROM jsonb_object_keys($2::jsonb) AS p
             ON CONFLICT DO NOTHING`,
            [file, JSON.stringify(base), f.rows[0]?.content_by ?? null],
          );
          return { vv: base, base_vv: base };
        });
        if (made !== 'changed') return made;
        if (attempt >= 2) throw new Error('содержимое файла меняется');
      }
    } finally {
      const next = u.creatingQueue.shift();
      if (next) next();
      else u.creating--;
    }
  }

  /** Догоняние: всё, что есть в журнале сверх `sub.sent` и не покрыто версией клиента, кусками и с учётом сокета. Когда
   * догнал — соединение снова получает рассылку напрямую. `announce` — конец подписки: клиенту сообщают версию сервера.
   *
   * Каждый проход — один запрос: записи и строка документа одним снимком базы. Записей между `sent` и первой найденной
   * нет — их убрало сжатие: клиент, чья версия не покрывает снимок, получает `snapshot-needed`. Рассылка включается до
   * последнего запроса, и он должен быть пуст: группа, зафиксированная между ними, либо видна запросу, либо уже идёт
   * рассылкой. */
  private async pump(d: ConnData, doc: string, sub: DocSub, announce: boolean): Promise<void> {
    if (sub.pumping) return;
    sub.pumping = true;
    const stale = () => d.closed || d.docs.get(doc) !== sub;
    try {
      let armed = false;
      for (;;) {
        if (stale()) return;
        const pass = await this.pool.query<{
          next_seq: string | null;
          vv: Record<string, number> | null;
          base_vv: Record<string, number> | null;
          seq: string | null;
          end_vv: Record<string, number> | null;
          size: number | null;
        }>(
          `SELECT s.next_seq, s.vv, s.base_vv, u.seq, u.end_vv, u.size
           FROM (SELECT 1) AS one
           LEFT JOIN doc_state s ON s.file_id = $1 AND s.doc = $2
           LEFT JOIN LATERAL (SELECT seq, end_vv, octet_length(bytes) AS size FROM doc_update
                              WHERE file_id = $1 AND doc = $2 AND seq > $3 ORDER BY seq LIMIT $4) u ON true
           ORDER BY u.seq`,
          [d.file, doc, sub.sent, this.limits.pumpRows],
        );
        if (stale()) return;
        // Рассылка отстала, пока шёл запрос: проход не последний.
        if (armed && !sub.live) armed = false;
        const head = pass.rows[0];
        // Пока шёл запрос, рассылка могла отправить часть найденного.
        const rows = pass.rows.filter((r) => r.seq !== null && Number(r.seq) > sub.sent);
        const last = head.next_seq === null ? 0 : Number(head.next_seq) - 1;
        const first = rows.length > 0 ? Number(rows[0].seq) : last + 1;
        if (first > sub.sent + 1) {
          if (!versionIncludes(sub.version, toMap(head.base_vv ?? {}))) {
            this.needSnapshot(d, doc);
            return;
          }
          // Клиент знает всё, что вошло в снимок.
          sub.sent = first - 1;
        }
        if (rows.length === 0) {
          if (armed) {
            d.lagSince = 0;
            if (announce) this.send(d, { t: 'subscribed', doc, seq: sub.sent, version: encodeVersion(toMap(head.vv ?? {})) });
            return;
          }
          sub.live = true;
          armed = true;
          continue;
        }
        sub.live = false;
        armed = false;
        const need: number[] = [];
        let bytes = 0;
        let prev = sub.sent;
        for (const r of rows) {
          // Дыра посреди прохода: сжатие оставило пропущенную запись, а записи за ней убрало. Проход кончается на ней, следующий
          // увидит дыру первой и сверит снимок.
          if (Number(r.seq) !== prev + 1) break;
          prev = Number(r.seq);
          const end = r.end_vv ?? {};
          // Клиент уже знает: не шлём, но и не останавливаемся.
          if (versionIncludes(sub.version, toMap(end))) {
            if (need.length === 0) sub.sent = Number(r.seq);
            continue;
          }
          if (need.length > 0 && bytes + (r.size ?? 0) > PUMP_BYTES) break;
          need.push(Number(r.seq));
          bytes += r.size ?? 0;
        }
        if (need.length === 0) continue;
        const got = await this.pool.query<{ seq: string; bytes: Uint8Array; end_vv: Record<string, number> }>(
          'SELECT seq, bytes, end_vv FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq = ANY($3::bigint[]) ORDER BY seq',
          [d.file, doc, need],
        );
        // Сжатие убрало записи между запросами: следующий проход увидит дыру и сверит снимок.
        for (const [i, r] of got.rows.entries()) {
          if (Number(r.seq) !== need[i]) break;
          await this.whenWritable(d);
          if (stale()) return;
          if (!this.send(d, { t: 'updates', doc, seq: Number(r.seq), bytes: r.bytes })) return;
          sub.sent = Number(r.seq);
          merge(sub.version, r.end_vv);
        }
        // Рядом с `sent` могли лежать записи, которые клиент знает: следующий запрос их пропустит.
      }
    } finally {
      sub.pumping = false;
    }
  }

  /** Клиент отстал сильнее журнала: снимок вобрал правки, которых у него нет. Он подпишется заново после снимка. */
  private needSnapshot(d: ConnData, doc: string): void {
    d.docs.delete(doc);
    this.send(d, { t: 'snapshot-needed', doc, url: snapshotPath(d.file, doc) });
  }

  /** Роль соединений файла заново: снята — закрыть сразу, понижена — сообщить. Зовут маршруты после правки доступа;
   * раз в `recheckMs` — на случай изменений мимо маршрутов. */
  async recheckFile(file: string): Promise<void> {
    await this.recheck([file]);
  }

  /** Все пары «файл, человек» открытых соединений — одним запросом. */
  private async recheck(files: string[]): Promise<void> {
    const pairs = new Map<string, { file: string; user: string }>();
    for (const file of files) for (const d of this.conns.get(file) ?? []) pairs.set(`${file}/${d.user}`, { file, user: d.user });
    if (pairs.size === 0) return;
    const list = [...pairs.values()];
    const { rows } = await this.pool.query<{ f: string; u: string; rank: number | null; member: number | null; version: string | null }>(
      `SELECT f, u, file_role_rank(f, u) AS rank, file_member_rank(f, u) AS member, (SELECT version FROM file WHERE id = f) AS version
       FROM unnest($1::uuid[], $2::text[]) AS x(f, u)`,
      [list.map((p) => p.file), list.map((p) => p.user)],
    );
    const by = new Map(rows.map((r) => [`${r.f}/${r.u}`, r]));
    for (const file of files) {
      for (const d of this.conns.get(file) ?? []) {
        const r = by.get(`${file}/${d.user}`);
        // Соединение открыто, пока шёл запрос: его роль проверена при открытии, следующая сверка его увидит.
        if (!r) continue;
        const role = RANK_ROLE[r.rank ?? 0];
        if (!role) {
          this.revoke(d, 'access');
          continue;
        }
        if (role !== d.role) {
          d.role = role;
          this.send(d, { t: 'role', role, canShare: (r.member ?? 0) >= ROLE_RANK.editor, version: Number(r.version ?? 0) });
          // Понижение посреди жеста: превью у других снимается сразу.
          this.presence.roleChanged(d);
        }
      }
    }
  }

  /** Команды, проекты и таймер: меняется доступ ко многим файлам сразу. Запуски сливаются: пока идёт сверка, следующие вызовы
   * дают ещё одну после неё, а не каждый свою. Промис — конец сверки, он не отказывает. */
  recheckAll(): Promise<void> {
    if (this.rechecking) {
      this.recheckAgain = true;
      return this.rechecking;
    }
    this.rechecking = (async () => {
      try {
        do {
          this.recheckAgain = false;
          await this.recheck([...this.conns.keys()]).catch(report);
        } while (this.recheckAgain && !this.stopped);
      } finally {
        this.rechecking = null;
      }
    })();
    return this.rechecking;
  }

  private revoke(d: ConnData, reason: RevokeReason): void {
    this.send(d, { t: 'access-revoked', reason });
    d.ws?.close(reason === 'session' ? SYNC_CLOSE.SESSION : SYNC_CLOSE.REVOKED, reason);
  }

  /** Выход на этом устройстве: соединения этой сессии закрываются сразу. */
  sessionEnded(session: string): void {
    for (const set of this.conns.values()) for (const d of set) if (d.session === session) this.revoke(d, 'session');
  }

  /** Выход на всех устройствах. */
  userSignedOut(user: string): void {
    for (const set of this.conns.values()) for (const d of set) if (d.user === user) this.revoke(d, 'session');
  }

  /** Сессия истекла или снята мимо сервера. Сбой Valkey — не повод выгонять людей: проверим в следующий раз. */
  private async checkSessions(): Promise<void> {
    const seen = new Map<string, boolean>();
    for (const set of this.conns.values()) {
      for (const d of set) {
        let alive = seen.get(d.cookie);
        if (alive === undefined) {
          const s = await this.auth.api.getSession({ headers: new Headers({ cookie: d.cookie }), query: { disableRefresh: true } });
          alive = !!s && s.session.id === d.session;
          seen.set(d.cookie, alive);
        }
        if (!alive) this.revoke(d, 'session');
      }
    }
  }

  private closeStalled(): void {
    const now = performance.now();
    for (const set of this.conns.values()) {
      for (const d of set) {
        if (d.lagSince && now - d.lagSince > this.limits.stalledMs) d.ws?.close(SYNC_CLOSE.SLOW, 'slow');
      }
    }
  }

  /** Файл заменён целиком (`PUT content` до начала журнала). Строк документов больше нет, а подписки знают прежние байты:
   * соединения закрываются, клиенты переподключатся и сверят версию с новым снимком. */
  forgetFile(file: string): void {
    this.validator.forget(file);
    for (const d of this.conns.get(file) ?? []) d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'replaced');
  }

  /** Событие комментариев файла (раздел 10.4): после фиксации изменения, из маршрута. Уходит соединениям с подтверждённой
   * ролью, у кого доступ есть сейчас: запрос ролей на проход рассылки — снятый доступ не получает события и до сверки
   * ролей. События файла рассылаются по возрастанию номера; пока идёт проход, следующие копятся и уходят следующим проходом
   * с одним запросом ролей на всех: медленная база не выстраивает очередь из запросов. Опоздавшее (номер не больше
   * разосланного) не шлётся — клиент, получивший больший номер с пропуском, догоняет по REST. Медленному сокету событие не
   * копится: пропуск и потом `gap`. */
  commentEvent(file: string, ev: CommentEvent): void {
    // Файл никто не держит открытым — слать некому, и порядок помнить незачем.
    if (this.stopped || !this.conns.has(file)) return;
    const queued = this.commentQueued.get(file);
    if (queued !== undefined && ev.seq > queued + 1) {
      // Раньше соседа: ждать его не дольше `COMMENT_REORDER_MS`, потом — как есть (клиент догонит по REST).
      let early = this.commentEarly.get(file);
      if (!early) {
        const timer = setTimeout(() => this.flushEarly(file, true), COMMENT_REORDER_MS);
        timer.unref?.();
        this.commentEarly.set(file, (early = { evs: [], timer }));
      }
      early.evs.push(ev);
      early.evs.sort((a, b) => a.seq - b.seq);
      if (early.evs.length >= COMMENT_EARLY_MAX) this.flushEarly(file, true);
      return;
    }
    this.queueComment(file, ev);
    this.flushEarly(file, false);
  }

  /** Ждущие своей очереди: подряд идущие — сейчас; `all` — все (срок вышел или их слишком много). */
  private flushEarly(file: string, all: boolean): void {
    const early = this.commentEarly.get(file);
    if (!early) return;
    while (early.evs.length > 0 && (all || early.evs[0].seq <= (this.commentQueued.get(file) ?? 0) + 1))
      this.queueComment(file, early.evs.shift()!);
    if (early.evs.length === 0) {
      clearTimeout(early.timer);
      this.commentEarly.delete(file);
    }
  }

  private queueComment(file: string, ev: CommentEvent): void {
    this.commentQueued.set(file, Math.max(ev.seq, this.commentQueued.get(file) ?? 0));
    let room = this.commentRooms.get(file);
    if (!room) this.commentRooms.set(file, (room = { queue: [], busy: false, overflow: false }));
    // Предел очереди файла (база зависла, событий поток): дальше — только последний номер, всем — `gap`.
    if (room.queue.length >= COMMENT_QUEUE_MAX) {
      room.overflow = true;
      room.queue = [];
    }
    room.queue.push(ev);
    if (room.busy) return;
    room.busy = true;
    const work = this.pumpComments(file, room);
    this.track(work);
  }

  private async pumpComments(file: string, room: { queue: CommentEvent[]; busy: boolean; overflow: boolean }): Promise<void> {
    try {
      while (room.queue.length > 0 && !this.stopped) {
        const batch = room.queue.splice(0);
        const overflow = room.overflow;
        room.overflow = false;
        await this.fanOutComments(file, batch, overflow).catch(report);
      }
    } finally {
      room.busy = false;
      if (room.queue.length === 0 && this.commentRooms.get(file) === room) this.commentRooms.delete(file);
    }
  }

  private async fanOutComments(file: string, batch: CommentEvent[], overflow: boolean): Promise<void> {
    const head = this.commentHeads.get(file) ?? 0;
    const evs = batch.filter((e) => e.seq > head).sort((a, b) => a.seq - b.seq);
    if (evs.length === 0) return;
    const targets = [...(this.conns.get(file) ?? [])].filter((d) => d.confirmed && !d.closed);
    if (targets.length === 0) return;
    const last = evs[evs.length - 1].seq;
    this.commentHeads.set(file, last);
    const users = [...new Set(targets.map((d) => d.user))];
    const { rows } = await this.pool.query<{ u: string; rank: number | null }>(
      'SELECT u, file_role_rank($1, u) AS rank FROM unnest($2::text[]) AS u',
      [file, users],
    );
    const allowed = new Set(rows.filter((r) => (r.rank ?? 0) > 0).map((r) => r.u));
    const frames = evs.map((ev) => encodeServerFrame({ t: 'comment', event: ev }));
    for (const d of targets) {
      if (d.closed || !allowed.has(d.user)) continue;
      // Очередь переполнялась — пропущено у всех: догнать по REST.
      if (overflow) d.commentGap = true;
      for (const [i, frame] of frames.entries()) {
        if (this.commentSlow(d)) {
          d.commentGap = true;
          this.commentSkipped += frames.length - i;
          break;
        }
        if (d.commentGap) {
          // Пропуск был: это событие и следующие клиент получит вместе с пропущенными, догоняя по REST.
          d.commentGap = false;
          this.send(d, { t: 'comment', event: { kind: 'gap', seq: last } });
          break;
        }
        this.sendBytes(d, frame);
      }
    }
  }

  private commentSlow(d: ConnData): boolean {
    return !!d.ws && d.ws.getBufferedAmount() > this.limits.commentHighWaterBytes;
  }

  /** Сокет освободился: пропустившему события — `gap` с последним номером файла. */
  private commentDrained(d: ConnData): void {
    if (!d.commentGap || d.closed || this.commentSlow(d)) return;
    d.commentGap = false;
    this.send(d, { t: 'comment', event: { kind: 'gap', seq: this.commentHeads.get(d.file) ?? 0 } });
  }

  /** Кто держит файл открытым: соединения с подтверждённой ролью. Им письма о комментариях не нужны. */
  openUsers(file: string): string[] {
    const out = new Set<string>();
    for (const d of this.conns.get(file) ?? []) if (d.confirmed && !d.closed) out.add(d.user);
    return [...out];
  }

  /** Открыт ли файл у кого-нибудь. */
  isOpen(file: string): boolean {
    return (this.conns.get(file)?.size ?? 0) > 0;
  }

  /** Сколько байт ждут в буферах сокетов: тесты и проба медленного читателя. */
  bufferedBytes(): number {
    let n = 0;
    for (const set of this.conns.values()) for (const d of set) n += d.ws?.getBufferedAmount() ?? 0;
    return n;
  }

  /** Сколько пиров хранят векторы подписок: тесты памяти на подписку. */
  versionEntries(): number {
    let n = 0;
    for (const set of this.conns.values()) for (const d of set) for (const sub of d.docs.values()) n += sub.version.size;
    return n;
  }

  /** Сколько соединений: тесты и проба памяти. */
  connections(): number {
    return this.total;
  }

  /** Остановка контейнера: принятое дописывается, соединения закрываются, клиенты переподключатся. */
  async stop(): Promise<void> {
    this.stopped = true;
    for (const early of this.commentEarly.values()) clearTimeout(early.timer);
    this.commentEarly.clear();
    for (const t of this.timers) clearInterval(t);
    this.presence.stop();
    await this.log.stop();
    for (const set of this.conns.values()) for (const d of set) d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'restart');
    // Обработчики, начатые до закрытия, дописывают своё раньше, чем пул закроется.
    while (this.inflight.size > 0) await Promise.allSettled(this.inflight);
    this.validator.stop();
  }
}

type ItemFrom = Item & { from?: ConnData; at?: number };

/** Счётчик пира в закодированном векторе (`encodeVersion`: пиры по возрастанию, по 12 байт) без разбора всего вектора. */
function counterOf(raw: Uint8Array, peer: string): number {
  const v = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  const want = BigInt(peer);
  let lo = 0;
  let hi = v.getUint32(0, true) - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >>> 1;
    const at = 4 + mid * 12;
    const p = v.getBigUint64(at, true);
    if (p === want) return v.getUint32(at + 8, true);
    if (p < want) lo = mid + 1;
    else hi = mid - 1;
  }
  return 0;
}

function merge(version: VersionMap, end: Record<string, number>): void {
  for (const [peer, counter] of Object.entries(end)) version.set(peer, Math.max(version.get(peer) ?? 0, counter));
}

function report(e: unknown): void {
  console.error('Синхронизация:', e instanceof Error ? e.message : e);
}

function tooMany(ms: number): Response {
  return new Response(null, { status: 429, headers: { 'Retry-After': String(Math.ceil(ms / 1000)) } });
}

/** Обработчик сокета не должен ронять процесс. */
function guard(work: () => void): void {
  try {
    work();
  } catch (e) {
    report(e);
  }
}
