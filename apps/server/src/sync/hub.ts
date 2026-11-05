// WebSocket файлов (спека подсистемы 4, разделы 7.1–7.3): открытие с проверкой `Origin`, сессии и роли, кадры с
// пределами, порядок «проверка → журнал → ack и рассылка», догоняние с учётом медленного сокета. Сервер не падает ни
// от сбоя базы, ни от чужого клиента: всякий обработчик под `try`, очереди и память на соединение ограничены.

import {
  encodeServerFrame,
  encodeVersion,
  parseClientFrame,
  parseVersion,
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
  version: VersionMap;
  /** Последняя запись, которую соединение уже получило. */
  sent: number;
  /** `true` — рассылка идёт напрямую; иначе соединение догоняет из базы. */
  live: boolean;
  pumping: boolean;
  /** Подписку повторили: идущий проход заканчивается. */
  generation: number;
}

export interface SyncLimits {
  framesPerSecond: number;
  framesBurst: number;
  userFramesPerSecond: number;
  userFramesBurst: number;
  userBytesPerSecond: number;
  userBytesBurst: number;
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
  log: Partial<LogLimits>;
}

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
  connectionsPerUser: 64,
  connectionsTotal: 20_000,
  subscribesPerConnection: 4096,
  highWaterBytes: 1024 * 1024,
  stalledMs: 120_000,
  strikes: 16,
  helloMs: 15_000,
  recheckMs: 30_000,
  sessionCheckMs: 60_000,
  log: {},
};

/** Столько записей журнала за один запрос догоняния и столько их байт. */
const PUMP_ROWS = 512;
const PUMP_BYTES = 8 * 1024 * 1024;

class Bucket {
  private tokens: number;
  private last = performance.now();
  constructor(
    private readonly rate: number,
    private readonly burst: number,
  ) {
    this.tokens = burst;
  }
  /** Взять `n`; не хватает — сколько миллисекунд ждать. */
  take(n: number): number {
    const now = performance.now();
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
    if (this.tokens >= n) {
      this.tokens -= n;
      return 0;
    }
    return Math.ceil(((n - this.tokens) / this.rate) * 1000);
  }
}

interface UserState {
  frames: Bucket;
  bytes: Bucket;
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
    this.timers.push(setInterval(() => this.closeStalled(), Math.min(5000, this.limits.stalledMs)));
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
    if (this.total >= this.limits.connectionsTotal || (this.users.get(session.userId)?.conns ?? 0) >= this.limits.connectionsPerUser) {
      return new Response(null, { status: 429, headers: { 'Retry-After': '5' } });
    }
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

  private async role(file: string, user: string): Promise<FileRole | null> {
    const { rows } = await this.pool.query<{ rank: number | null }>('SELECT file_role_rank($1, $2) AS rank', [file, user]);
    return RANK_ROLE[rows[0]?.rank ?? 0];
  }

  /** Настройка `websocket` для `Bun.serve`. */
  readonly websocket: Bun.WebSocketHandler<ConnData> = {
    maxPayloadLength: 16 * 1024 * 1024 + 1024,
    idleTimeout: 120,
    sendPings: true,
    // Память на медленного читателя: дальше сокет не копит, соединение догонит из базы.
    backpressureLimit: 8 * 1024 * 1024,
    closeOnBackpressureLimit: false,
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
    let u = this.users.get(d.user);
    if (!u) {
      u = {
        frames: new Bucket(this.limits.userFramesPerSecond, this.limits.userFramesBurst),
        bytes: new Bucket(this.limits.userBytesPerSecond, this.limits.userBytesBurst),
        conns: 0,
      };
      this.users.set(d.user, u);
    }
    u.conns++;
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
    if (u && --u.conns === 0) this.users.delete(d.user);
    const set = this.conns.get(d.file);
    set?.delete(d);
    if (set?.size === 0) {
      this.conns.delete(d.file);
      this.onLastLeft(d.file);
    }
  }

  private send(d: ConnData, frame: ServerFrame): void {
    this.sendBytes(d, encodeServerFrame(frame));
  }

  private sendBytes(d: ConnData, bytes: Uint8Array): number {
    if (d.closed || !d.ws) return 0;
    return d.ws.sendBinary(bytes);
  }

  private strike(d: ConnData): void {
    if (++d.strikes >= this.limits.strikes) d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'bad frames');
  }

  private received(ws: Bun.ServerWebSocket<ConnData>, message: string | Buffer): void {
    const d = ws.data;
    if (typeof message === 'string') {
      ws.close(SYNC_CLOSE.BAD_FRAME, 'text');
      return;
    }
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
    const u = this.users.get(d.user)!;
    const wait = Math.max(d.frames.take(1), u.frames.take(1), frame.t === 'update' ? u.bytes.take(frame.bytes.length) : 0);
    if (wait > 0) {
      // Предел частоты — пауза: кадр не принят, клиент пришлёт его снова. Подписка и правка равны перед ним.
      this.send(d, { t: 'pause', ms: Math.min(wait, 30_000) });
      return;
    }
    switch (frame.t) {
      case 'hello':
        this.hello(d, frame.protocol, frame.schema);
        break;
      case 'subscribe':
        void this.subscribe(d, frame.doc, frame.version).catch((e: unknown) => {
          console.error('Подписка:', e instanceof Error ? e.message : e);
          // База недоступна: соединение закрывается, клиент переподключится и подпишется заново.
          d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
        });
        break;
      case 'update':
        void this.update(d, frame.doc, frame.clientSeq, frame.bytes).catch((e: unknown) => {
          console.error('Приём пачки:', e instanceof Error ? e.message : e);
          this.refuse(d, frame.doc, frame.clientSeq, 'UNAVAILABLE');
        });
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
    void this.pool
      .query<{ version: string; member: number; schema_version: number }>(
        `UPDATE file SET schema_version = GREATEST(schema_version, $3) WHERE id = $1
         RETURNING version, file_member_rank(id, $2) AS member, schema_version`,
        [d.file, d.user, Math.min(schema, 1_000_000)],
      )
      .then(({ rows }) => {
        if (rows[0])
          this.send(d, { t: 'role', role: d.role, canShare: rows[0].member >= ROLE_RANK.editor, version: Number(rows[0].version) });
      })
      .catch(report);
  }

  private refuse(d: ConnData, doc: string, clientSeq: number, code: SyncRefusal): void {
    this.send(d, { t: 'refused', doc, clientSeq, code });
  }

  private async update(d: ConnData, doc: string, clientSeq: number, frame: Uint8Array): Promise<void> {
    // Буфер кадра живёт только на время обработчика сокета: дальше — своя копия.
    const bytes = frame.slice();
    // Читатель не пишет: и по последней роли соединения, и в транзакции записи.
    if (ROLE_RANK[d.role] < ROLE_RANK.editor) {
      this.strike(d);
      this.refuse(d, doc, clientSeq, 'FORBIDDEN');
      return;
    }
    if (!d.docs.has(doc)) {
      this.strike(d);
      this.refuse(d, doc, clientSeq, 'GAP');
      return;
    }
    const verdict = await this.validator.validate(d.file, doc, bytes);
    if (verdict.kind === 'bad') {
      this.strike(d);
      this.refuse(d, doc, clientSeq, 'BAD_UPDATE');
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
    const item: Item = { file: d.file, doc, user: d.user, bytes, start: verdict.start, end: verdict.end };
    (item as ItemFrom).from = d;
    const outcome = await this.log.submit(item);
    if (outcome.kind === 'refused') {
      this.refuse(d, doc, clientSeq, outcome.code);
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
        for (const [peer, counter] of Object.entries(item.end)) sub.version.set(peer, Math.max(sub.version.get(peer) ?? 0, counter));
        continue;
      }
      if (!sub.live) continue;
      // Пропуск в рассылке (соединение отстало) — только через догоняние из базы.
      if (sub.sent !== seq - 1 || this.slow(d)) {
        this.goLag(d, item.doc, sub);
        continue;
      }
      frame ??= encodeServerFrame({ t: 'updates', doc: item.doc, seq, bytes: item.bytes });
      const sent = this.sendBytes(d, frame);
      if (sent === 0) {
        // Сокет отказал: кадр не ушёл, догонит из базы.
        this.goLag(d, item.doc, sub);
        continue;
      }
      sub.sent = seq;
    }
  }

  private slow(d: ConnData): boolean {
    return !!d.ws && d.ws.getBufferedAmount() > this.limits.highWaterBytes;
  }

  private goLag(d: ConnData, doc: string, sub: DocSub): void {
    sub.live = false;
    if (!d.lagSince) d.lagSince = performance.now();
    if (!sub.pumping) void this.pump(d, doc, sub, sub.generation, false).catch((e: unknown) => this.pumpFailed(d, e));
  }

  private pumpFailed(d: ConnData, e: unknown): void {
    console.error('Догоняние:', e instanceof Error ? e.message : e);
    d.ws?.close(SYNC_CLOSE.GOING_AWAY, 'unavailable');
  }

  /** Сокет освободился; не занят — сразу. */
  private whenWritable(d: ConnData): Promise<void> {
    if (d.closed || !this.slow(d)) return Promise.resolve();
    if (!d.lagSince) d.lagSince = performance.now();
    return new Promise((resolve) => d.drained.push(resolve));
  }

  private async subscribe(d: ConnData, doc: string, raw: Uint8Array): Promise<void> {
    const version = parseVersion(raw);
    if (!version) return;
    const old = d.docs.get(doc);
    if (!old && d.docs.size >= this.limits.subscribesPerConnection) {
      d.ws?.close(SYNC_CLOSE.BAD_FRAME, 'too many docs');
      return;
    }
    const sub: DocSub = { version, sent: 0, live: false, pumping: false, generation: (old?.generation ?? 0) + 1 };
    d.docs.set(doc, sub);
    const state = await this.docState(d.file, doc);
    if (d.closed || d.docs.get(doc) !== sub) return;
    // Клиент отстал сильнее журнала: снимок вобрал правки, которых у него нет.
    if (state && !versionIncludes(version, toMap(state.base_vv))) {
      d.docs.delete(doc);
      this.send(d, { t: 'snapshot-needed', doc, url: snapshotPath(d.file, doc) });
      return;
    }
    await this.pump(d, doc, sub, sub.generation, true);
  }

  /** Строка документа; первая подписка заводит её с версией снимка файла. Читатель новых строк не заводит: у документа
   * без снимка (страница, в которую ещё не писали) журнала нет, и версия у него пустая. */
  private async docState(file: string, doc: string): Promise<{ base_vv: Record<string, number> } | null> {
    const found = await this.pool.query<{ base_vv: Record<string, number> }>(
      'SELECT base_vv FROM doc_state WHERE file_id = $1 AND doc = $2',
      [file, doc],
    );
    if (found.rows[0]) return found.rows[0];
    const { rows } = await this.pool.query<{ content_key: string | null }>('SELECT content_key FROM file WHERE id = $1', [file]);
    const snapshot = await readDocSnapshot(this.blobs, rows[0]?.content_key ?? null, doc);
    if (!snapshot) return null;
    const base = await this.validator.baseVersion(snapshot);
    await this.pool.query(
      `INSERT INTO doc_state (file_id, doc, vv, base_vv) VALUES ($1, $2, $3::jsonb, $3::jsonb) ON CONFLICT DO NOTHING`,
      [file, doc, JSON.stringify(base)],
    );
    return { base_vv: base };
  }

  /** Догоняние: всё, что есть в журнале сверх `sub.sent` и не покрыто версией клиента, кусками и с учётом сокета. Когда
   * догнал — соединение снова получает рассылку напрямую. `announce` — конец подписки: клиенту сообщают версию сервера. */
  private async pump(d: ConnData, doc: string, sub: DocSub, generation: number, announce: boolean): Promise<void> {
    if (sub.pumping) return;
    sub.pumping = true;
    try {
      for (;;) {
        if (d.closed || sub.generation !== generation || d.docs.get(doc) !== sub) return;
        const { rows } = await this.pool.query<{ seq: string; end_vv: Record<string, number>; size: number }>(
          `SELECT seq, end_vv, octet_length(bytes) AS size FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq > $3 ORDER BY seq LIMIT $4`,
          [d.file, doc, sub.sent, PUMP_ROWS],
        );
        if (rows.length === 0) {
          const head = await this.pool.query<{ next_seq: string; vv: Record<string, number> }>(
            'SELECT next_seq, vv FROM doc_state WHERE file_id = $1 AND doc = $2',
            [d.file, doc],
          );
          if (d.closed || sub.generation !== generation) return;
          const seq = head.rows[0] ? Number(head.rows[0].next_seq) - 1 : 0;
          // Журнал пуст после сжатия, а версия клиента покрывает снимок: писать нечего.
          if (seq > sub.sent) sub.sent = seq;
          sub.live = true;
          d.lagSince = 0;
          if (announce) {
            this.send(d, { t: 'subscribed', doc, seq, version: encodeVersion(toMap(head.rows[0]?.vv ?? {})) });
          }
          return;
        }
        const need: number[] = [];
        let bytes = 0;
        for (const r of rows) {
          // Клиент уже знает: не шлём, но и не останавливаемся.
          if (versionIncludes(sub.version, toMap(r.end_vv))) {
            if (need.length === 0) sub.sent = Number(r.seq);
            continue;
          }
          if (need.length > 0 && bytes + r.size > PUMP_BYTES) break;
          need.push(Number(r.seq));
          bytes += r.size;
        }
        if (need.length === 0) continue;
        const got = await this.pool.query<{ seq: string; bytes: Uint8Array }>(
          'SELECT seq, bytes FROM doc_update WHERE file_id = $1 AND doc = $2 AND seq = ANY($3::bigint[]) ORDER BY seq',
          [d.file, doc, need],
        );
        for (const r of got.rows) {
          await this.whenWritable(d);
          if (d.closed || sub.generation !== generation) return;
          this.send(d, { t: 'updates', doc, seq: Number(r.seq), bytes: r.bytes });
          sub.sent = Number(r.seq);
        }
        // Сжатие убрало записи посреди прохода: всё, что до них, вошло в снимок. Клиент подпишется заново и получит его.
        if (got.rows.length < need.length) {
          d.docs.delete(doc);
          this.send(d, { t: 'snapshot-needed', doc, url: snapshotPath(d.file, doc) });
          return;
        }
        // Рядом с `sent` могли лежать записи, которые клиент знает: следующий запрос их пропустит.
      }
    } finally {
      sub.pumping = false;
    }
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
      const role = RANK_ROLE[r?.rank ?? 0];
      if (!role) {
        this.revoke(d, 'access');
        continue;
      }
      if (role !== d.role) {
        d.role = role;
        this.send(d, { t: 'role', role, canShare: (r?.member ?? 0) >= ROLE_RANK.editor, version: Number(r?.version ?? 0) });
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

  /** Сколько байт ждут в буферах сокетов: тесты и проба медленного читателя. */
  bufferedBytes(): number {
    let n = 0;
    for (const set of this.conns.values()) for (const d of set) n += d.ws?.getBufferedAmount() ?? 0;
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
    this.validator.stop();
  }
}

type ItemFrom = Item & { from?: ConnData };

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
