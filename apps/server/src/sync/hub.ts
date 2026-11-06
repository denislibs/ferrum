// WebSocket файлов (спека подсистемы 4, разделы 7.1–7.3): открытие с проверкой `Origin`, сессии и роли, кадры с
// пределами, порядок «проверка → журнал → ack и рассылка», догоняние с учётом медленного сокета. Сервер не падает ни
// от сбоя базы, ни от чужого клиента: всякий обработчик под `try`, очереди и память на соединение ограничены.

import {
  encodeServerFrame,
  encodeVersion,
  MAX_SYNC_FRAME_BYTES,
  parseClientFrame,
  RANK_ROLE,
  ROLE_RANK,
  snapshotPath,
  SYNC_CLOSE,
  SYNC_PROTOCOL,
  versionIncludes,
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
import { readDocSnapshot } from './container-read';
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
  connectionsPerUser: number;
  connectionsTotal: number;
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
  connectionsPerUser: 64,
  connectionsTotal: 20_000,
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
};

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
}

const toMap = (o: Record<string, number>): VersionMap => new Map(Object.entries(o));

export class SyncHub {
  readonly validator: Validator;
  readonly log: UpdateLog;
  private readonly limits: SyncLimits;
  private readonly conns = new Map<string, Set<ConnData>>();
  private readonly users = new Map<string, UserState>();
  private total = 0;
  private seq = 0;
  private readonly timers: Timer[] = [];
  private stopped = false;
  /** Идущие обработчики кадров и догоняния: остановка ждёт их, прежде чем отдать пул. */
  private readonly inflight = new Set<Promise<unknown>>();
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
    this.log.onCommitted = (items, states) => {
      for (const { item, seq } of items) this.broadcast(item, seq);
      for (const s of states) this.onLog(s.file, s.doc, s.logCount, s.logBytes);
    };
    this.timers.push(setInterval(() => void this.recheckEverything().catch(report), this.limits.recheckMs));
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

  /** Проверка перед открытием: `Origin` и живая сессия (`authorizeUpgrade`), вкладка за того же человека, роль, предел соединений. */
  async upgrade(req: Request, server: Bun.Server<ConnData>, file: string): Promise<Response | undefined> {
    if (this.stopped) return new Response(null, { status: 503 });
    let session: UpgradeSession | null;
    try {
      session = await authorizeUpgrade(req, this.auth, this.publicUrl);
    } catch {
      return new Response(null, { status: 503 });
    }
    if (!session) return new Response(null, { status: 401 });
    // Вкладка работает за другого человека (вход сменили в другой вкладке): правки X не пишутся под Y.
    const expected = new URL(req.url).searchParams.get('user');
    if (expected !== session.userId) return new Response(null, { status: 401 });
    let role: FileRole | null;
    try {
      role = await this.role(file, session.userId);
    } catch {
      return new Response(null, { status: 503 });
    }
    if (!role) return new Response(null, { status: 404 });
    const u = this.user(session.userId);
    if (this.total >= this.limits.connectionsTotal || u.conns >= this.limits.connectionsPerUser) {
      return new Response(null, { status: 429, headers: { 'Retry-After': '5' } });
    }
    // Грубые нарушения исчерпали бакет: до его пополнения соединений нет.
    const banned = u.faults.peek(1);
    if (banned > 0) return new Response(null, { status: 429, headers: { 'Retry-After': String(Math.ceil(banned / 1000)) } });
    const wait = u.opens.take(1);
    if (wait > 0) return new Response(null, { status: 429, headers: { 'Retry-After': String(Math.ceil(wait / 1000)) } });
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
    };
    if (!server.upgrade(req, { data })) return new Response(null, { status: 400 });
    return undefined;
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
      };
      this.users.set(id, u);
    }
    return u;
  }

  /** Человек без соединений с полными бакетами ничем не отличается от нового. */
  private sweepUsers(): void {
    for (const [id, u] of this.users) {
      if (u.conns === 0 && u.frames.full() && u.bytes.full() && u.opens.full() && u.subscribeBytes.full() && u.faults.full())
        this.users.delete(id);
    }
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
    const set = this.conns.get(d.file);
    set?.delete(d);
    if (set?.size === 0) {
      this.conns.delete(d.file);
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
    const frame = parseClientFrame(message);
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
      case 'ephemeral':
        // Присутствие — этап 4: кадр известен, сервер его пока не рассылает.
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
        .query<{ version: string; member: number; rank: number | null }>(
          `WITH f AS (SELECT id, version, file_role_rank(id, $2) AS rank, file_member_rank(id, $2) AS member FROM file WHERE id = $1),
                up AS (UPDATE file SET schema_version = $3 FROM f
                       WHERE file.id = f.id AND f.rank >= $4 AND file.schema_version < $3 RETURNING 1)
           SELECT version, member, rank FROM f`,
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
          this.send(d, { t: 'role', role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
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
      // Чужой пир настоящий клиент не шлёт, а подделка отравляет цепочку проверки соавторам: их пачки получают `GAP`.
      if (outcome.code === 'FOREIGN_PEER') this.fault(d, 'foreign peer');
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
    // кадра — своя копия: буфер сокета живёт только на время обработчика. Разбор уже проверен `parseClientFrame`.
    const raw = frame.slice();
    const old = d.docs.get(doc);
    if (!old && d.docs.size >= this.limits.subscribesPerConnection) {
      d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'too many docs');
      return;
    }
    const sub: DocSub = { version: new Map(), sent: 0, live: false, pumping: false, ready: false };
    d.docs.set(doc, sub);
    const state = await this.docState(d.file, doc);
    if (d.closed || d.docs.get(doc) !== sub) return;
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

  /** Строка документа; первая подписка заводит её с версией снимка файла. Читатель новых строк не заводит: у документа
   * без снимка (страница, в которую ещё не писали) журнала нет, и версия у него пустая. */
  private async docState(file: string, doc: string): Promise<{ vv: Record<string, number>; base_vv: Record<string, number> } | null> {
    const found = await this.pool.query<{ vv: Record<string, number>; base_vv: Record<string, number> }>(
      'SELECT vv, base_vv FROM doc_state WHERE file_id = $1 AND doc = $2',
      [file, doc],
    );
    if (found.rows[0]) return found.rows[0];
    const { rows } = await this.pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
    const snapshot = await readDocSnapshot(this.blobs, rows[0]?.content_key ?? null, doc);
    if (!snapshot) return null;
    // Сбой проверки — не пустая версия: строка с пустой версией дала бы `GAP` на каждую правку. Подписка повторится.
    const base = await this.validator.baseVersion(snapshot);
    if (!base) throw new Error('проверка недоступна');
    await this.pool.query(
      `INSERT INTO doc_state (file_id, doc, vv, base_vv) VALUES ($1, $2, $3::jsonb, $3::jsonb) ON CONFLICT DO NOTHING`,
      [file, doc, JSON.stringify(base)],
    );
    return { vv: base, base_vv: base };
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
    const set = this.conns.get(file);
    if (!set || set.size === 0) return;
    const users = [...new Set([...set].map((d) => d.user))];
    const { rows } = await this.pool.query<{ u: string; rank: number | null; member: number | null; version: string | null }>(
      `SELECT u, file_role_rank($1, u) AS rank, file_member_rank($1, u) AS member,
              (SELECT version FROM file WHERE id = $1) AS version
       FROM unnest($2::text[]) AS x(u)`,
      [file, users],
    );
    const by = new Map(rows.map((r) => [r.u, r]));
    for (const d of set) {
      const r = by.get(d.user);
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
      }
    }
  }

  private async recheckEverything(): Promise<void> {
    for (const file of this.conns.keys()) await this.recheckFile(file);
  }

  /** Команды и проекты: меняется доступ ко многим файлам сразу. */
  recheckAll(): void {
    void this.recheckEverything().catch(report);
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

  /** Файл заменён целиком (`PUT content` до начала журнала). */
  forgetFile(file: string): void {
    this.validator.forget(file);
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
    for (const t of this.timers) clearInterval(t);
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

/** Обработчик сокета не должен ронять процесс. */
function guard(work: () => void): void {
  try {
    work();
  } catch (e) {
    report(e);
  }
}
