// Кадры WebSocket файла (спека подсистемы 4, раздел 7.2): двоичные, разбор не доверяет данным, неверное — `null`.
// Первый байт — вид кадра, числа — LE, строка — длина u8 и UTF-8. Документ — `root` или `TreeID` страницы.

import type { FileRole } from './files';
import {
  parsePartBody,
  parsePresenceBatch,
  parsePresenceBody,
  PRESENCE_BATCH_MAX,
  PRESENCE_FRAME_MAX,
  type PresenceItem,
  type PresencePart,
  type PresenceState,
} from './presence';

/** Растёт при несовместимой смене кадров: старый клиент получает `please-reload`. 2 — присутствие (этап 4): кадров
 * `presence` прежний клиент не знает и счёл бы соединение испорченным. */
export const SYNC_PROTOCOL = 2;
/** Предел кадра по спеке, раздел 7.3. Вся полезная нагрузка — байты Loro, заголовок сверху не считается. */
export const MAX_SYNC_UPDATE_BYTES = 16 * 1024 * 1024;
/** Сервер закрывает кадр больше этого, не читая его. */
export const MAX_SYNC_FRAME_BYTES = MAX_SYNC_UPDATE_BYTES + 1024;
/** Вектор версий: столько сеансов правки на один документ не набирается, а чужой кадр не задаёт размер разбора. */
export const MAX_VERSION_PEERS = 65_536;
const MAX_DOC_BYTES = 40;
/** Документ — `root` или `TreeID` страницы `counter@peer` в той же записи, что у Loro: без ведущих нулей, иначе один документ жил бы
 * под разными ключами подписок, журнала и пределов. Диапазоны — ниже, в `isSyncDoc`. */
const DOC_RE = /^(root|(0|[1-9]\d{0,9})@(0|[1-9]\d{0,19}))$/;
/** Счётчик `TreeID` у Loro — `i32`, пир — `u64`. */
const MAX_TREE_COUNTER = 0x7fffffff;
const MAX_PEER = 2n ** 64n - 1n;

/** Адрес соединения файла: `?user=` — кто в этой вкладке, как заголовок `USER_HEADER` у запросов. */
export const syncPath = (file: string, user: string): string => `/api/files/${file}/sync?user=${encodeURIComponent(user)}`;
export const snapshotPath = (file: string, doc: string): string => `/api/files/${file}/docs/${doc}/snapshot`;
/** Адрес снимка в `snapshot-needed`: только свой маршрут, клиент всё равно строит его сам из файла и документа. */
const SNAPSHOT_RE = /^\/api\/files\/[A-Za-z0-9_-]{1,64}\/docs\/([^/]+)\/snapshot$/;

/** Коды закрытия соединения: клиент по ним решает, пробовать ли снова. */
export const SYNC_CLOSE = {
  /** Сервер останавливается: переподключиться. */
  GOING_AWAY: 1001,
  TOO_BIG: 1009,
  /** Клиент нарушил протокол: повтор не поможет. */
  BAD_FRAME: 4400,
  /** Сессии больше нет или вкладка работает за другого человека. */
  SESSION: 4401,
  /** Доступа к файлу больше нет. */
  REVOKED: 4403,
  /** Нужна новая версия приложения. */
  RELOAD: 4426,
  /** Слишком много соединений. */
  TOO_MANY: 4429,
  /** Не успевает читать: догонит заново. */
  SLOW: 4408,
} as const;

/** Почему отказано в пачке. */
export const SYNC_REFUSALS = [
  /** Байты разобрать не удалось, или они не применяются. Повтор бессмыслен. */
  'BAD_UPDATE',
  /** Пачка вне вектора документа: клиент подписывается заново и шлёт от версии сервера. */
  'GAP',
  /** Роли не хватает. */
  'FORBIDDEN',
  /** Пачка пишет от чужого пира. */
  'FOREIGN_PEER',
  /** Журнал файла переполнен: сжатие отстало или не удаётся. Правки ждут у клиента. */
  'LOG_FULL',
  /** База недоступна: правка не записана, клиент подписывается заново и шлёт снова. */
  'UNAVAILABLE',
  /** В файле больше документов, чем держит сервер: новый документ не заводится, повтор не поможет. */
  'DOC_LIMIT',
] as const;
export type SyncRefusal = (typeof SYNC_REFUSALS)[number];

/** Почему доступ снят. */
export type RevokeReason = 'access' | 'session';

export type ClientFrame =
  | { t: 'hello'; protocol: number; schema: number }
  | { t: 'subscribe'; doc: string; version: Uint8Array }
  | { t: 'update'; doc: string; clientSeq: number; bytes: Uint8Array }
  /** Присутствие (спека, раздел 9): `body` — тело `encodePresenceBody`. При разборе `state` — разобранное тело, `null` —
   * тело неверно или больше 2 КБ: сервер такой кадр молча отбрасывает, а не рвёт соединение. */
  | { t: 'presence'; body: Uint8Array; state?: PresenceState | null }
  /** Часть выделения: тело `encodePartBody`, при разборе `part` — как `state` выше. */
  | { t: 'presence-part'; body: Uint8Array; part?: PresencePart | null }
  | { t: 'ping' };

export type ServerFrame =
  | { t: 'role'; role: FileRole; canShare: boolean; version: number }
  | { t: 'subscribed'; doc: string; seq: number; version: Uint8Array }
  | { t: 'updates'; doc: string; seq: number; bytes: Uint8Array }
  | { t: 'snapshot-needed'; doc: string; url: string }
  | { t: 'ack'; doc: string; clientSeq: number; seq: number }
  | { t: 'refused'; doc: string; clientSeq: number; code: SyncRefusal }
  /** Предел частоты: пауза, а не сбой. Кадр не принят и после неё будет отправлен снова. */
  | { t: 'pause'; ms: number }
  | { t: 'access-revoked'; reason: RevokeReason }
  | { t: 'please-reload' }
  | { t: 'pong' }
  /** Пачка присутствия других участников файла (`encodePresenceBatch`). */
  | { t: 'presence'; items: PresenceItem[] };

const KIND = {
  hello: 1,
  subscribe: 2,
  update: 3,
  presence: 4,
  ping: 5,
  'presence-part': 6,
  role: 16,
  subscribed: 17,
  updates: 18,
  'snapshot-needed': 19,
  ack: 20,
  refused: 21,
  pause: 22,
  'access-revoked': 23,
  'please-reload': 24,
  pong: 25,
  'presence-batch': 26,
} as const;

const ROLES: readonly FileRole[] = ['viewer', 'editor', 'owner'];
const REASONS: readonly RevokeReason[] = ['access', 'session'];
const utf8 = new TextEncoder();
/** `ignoreBOM`: BOM остаётся в строке и не проходит проверку, иначе `EF BB BF root` разбирался бы как `root`. */
const fromUtf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** Кодирование доверяет вызывающему, но не молча: число вне поля — ошибка здесь, а не другой кадр на проводе. */
function checkInt(v: number, max: number, what: string): void {
  if (!Number.isSafeInteger(v) || v < 0 || v > max) throw new RangeError(`${what}: ${v} вне 0..${max}`);
}

class Writer {
  private readonly parts: Uint8Array[] = [];
  private size = 0;
  private push(p: Uint8Array): this {
    this.parts.push(p);
    this.size += p.length;
    return this;
  }
  u8(v: number): this {
    checkInt(v, 0xff, 'u8');
    return this.push(Uint8Array.of(v));
  }
  u16(v: number): this {
    checkInt(v, 0xffff, 'u16');
    const b = new Uint8Array(2);
    new DataView(b.buffer).setUint16(0, v, true);
    return this.push(b);
  }
  u32(v: number): this {
    checkInt(v, 0xffffffff, 'u32');
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, true);
    return this.push(b);
  }
  /** Номер записи журнала: до 2^53, столько не набрать. */
  u64(v: number): this {
    checkInt(v, Number.MAX_SAFE_INTEGER, 'u64');
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(v), true);
    return this.push(b);
  }
  str(s: string): this {
    const b = utf8.encode(s);
    return this.u16(b.length).push(b);
  }
  doc(s: string): this {
    if (!isSyncDoc(s)) throw new RangeError(`документ: ${s}`);
    const b = utf8.encode(s);
    return this.u8(b.length).push(b);
  }
  bytes(b: Uint8Array): this {
    return this.push(b);
  }
  done(): Uint8Array<ArrayBuffer> {
    const out = new Uint8Array(this.size);
    let at = 0;
    for (const p of this.parts) {
      out.set(p, at);
      at += p.length;
    }
    return out;
  }
}

class Reader {
  private at = 1;
  private readonly view: DataView;
  constructor(private readonly b: Uint8Array) {
    this.view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  }
  private has(n: number): boolean {
    return this.at + n <= this.b.length;
  }
  u8(): number | null {
    if (!this.has(1)) return null;
    return this.b[this.at++];
  }
  u16(): number | null {
    if (!this.has(2)) return null;
    const v = this.view.getUint16(this.at, true);
    this.at += 2;
    return v;
  }
  u32(): number | null {
    if (!this.has(4)) return null;
    const v = this.view.getUint32(this.at, true);
    this.at += 4;
    return v;
  }
  u64(): number | null {
    if (!this.has(8)) return null;
    const v = this.view.getBigUint64(this.at, true);
    this.at += 8;
    return v <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(v) : null;
  }
  str(max: number): string | null {
    const n = this.u16();
    if (n === null || n > max || !this.has(n)) return null;
    return this.text(n);
  }
  doc(): string | null {
    const n = this.u8();
    if (n === null || n > MAX_DOC_BYTES || !this.has(n)) return null;
    const s = this.text(n);
    return s !== null && isSyncDoc(s) ? s : null;
  }
  private text(n: number): string | null {
    try {
      const s = fromUtf8.decode(this.b.subarray(this.at, this.at + n));
      this.at += n;
      return s;
    } catch {
      return null;
    }
  }
  rest(): Uint8Array {
    const r = this.b.subarray(this.at);
    this.at = this.b.length;
    return r;
  }
  done(): boolean {
    return this.at === this.b.length;
  }
}

export function isSyncDoc(s: string): boolean {
  if (!DOC_RE.test(s)) return false;
  if (s === 'root') return true;
  const [counter, peer] = s.split('@');
  return Number(counter) <= MAX_TREE_COUNTER && BigInt(peer) <= MAX_PEER;
}

export function encodeClientFrame(f: ClientFrame): Uint8Array<ArrayBuffer> {
  const w = new Writer().u8(KIND[f.t]);
  switch (f.t) {
    case 'hello':
      return w.u16(f.protocol).u32(f.schema).done();
    case 'subscribe':
      return w.doc(f.doc).bytes(f.version).done();
    case 'update':
      return w.doc(f.doc).u32(f.clientSeq).bytes(f.bytes).done();
    case 'presence':
    case 'presence-part':
      return w.bytes(f.body).done();
    case 'ping':
      return w.done();
  }
}

/** Пачка присутствия одному получателю: записи подряд, не больше `PRESENCE_BATCH_MAX`. */
export function encodePresenceBatch(records: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const w = new Writer().u8(KIND['presence-batch']);
  for (const r of records) w.bytes(r);
  const out = w.done();
  if (out.length - 1 > PRESENCE_BATCH_MAX) throw new RangeError('пачка присутствия больше предела');
  return out;
}

export function encodeServerFrame(f: ServerFrame): Uint8Array<ArrayBuffer> {
  const w = new Writer().u8(KIND[f.t]);
  switch (f.t) {
    case 'role':
      return w
        .u8(ROLES.indexOf(f.role))
        .u8(f.canShare ? 1 : 0)
        .u64(f.version)
        .done();
    case 'subscribed':
      return w.doc(f.doc).u64(f.seq).bytes(f.version).done();
    case 'updates':
      return w.doc(f.doc).u64(f.seq).bytes(f.bytes).done();
    case 'snapshot-needed':
      return w.doc(f.doc).str(f.url).done();
    case 'ack':
      return w.doc(f.doc).u32(f.clientSeq).u64(f.seq).done();
    case 'refused':
      return w.doc(f.doc).u32(f.clientSeq).u8(SYNC_REFUSALS.indexOf(f.code)).done();
    case 'pause':
      return w.u32(f.ms).done();
    case 'access-revoked':
      return w.u8(REASONS.indexOf(f.reason)).done();
    case 'please-reload':
    case 'pong':
      return w.done();
    case 'presence':
      throw new RangeError('пачку присутствия собирает encodePresenceBatch');
  }
}

/** Разбор кадра клиента на сервере. Байты пачки — вид внутрь кадра, без копии. */
export function parseClientFrame(b: Uint8Array): ClientFrame | null {
  if (b.length === 0 || b.length > MAX_SYNC_FRAME_BYTES) return null;
  const r = new Reader(b);
  switch (b[0]) {
    case KIND.hello: {
      const protocol = r.u16();
      const schema = r.u32();
      return protocol !== null && schema !== null && r.done() ? { t: 'hello', protocol, schema } : null;
    }
    case KIND.subscribe: {
      const doc = r.doc();
      if (doc === null) return null;
      const version = r.rest();
      return validVersion(version) ? { t: 'subscribe', doc, version } : null;
    }
    case KIND.update: {
      const doc = r.doc();
      const clientSeq = r.u32();
      if (doc === null || clientSeq === null) return null;
      const bytes = r.rest();
      return bytes.length > 0 && bytes.length <= MAX_SYNC_UPDATE_BYTES ? { t: 'update', doc, clientSeq, bytes } : null;
    }
    case KIND.presence: {
      const body = r.rest();
      return { t: 'presence', body, state: b.length <= PRESENCE_FRAME_MAX ? parsePresenceBody(body) : null };
    }
    case KIND['presence-part']: {
      const body = r.rest();
      return { t: 'presence-part', body, part: b.length <= PRESENCE_FRAME_MAX ? parsePartBody(body) : null };
    }
    case KIND.ping:
      return r.done() ? { t: 'ping' } : null;
    default:
      return null;
  }
}

/** Разбор кадра сервера у клиента: сеть не доверенная, неверное — как обрыв. Пределы те же, что у кадров клиента: общий предел
 * кадра — запас поверх пределов каждого вида. */
export function parseServerFrame(b: Uint8Array): ServerFrame | null {
  if (b.length === 0 || b.length > MAX_SYNC_FRAME_BYTES) return null;
  const r = new Reader(b);
  switch (b[0]) {
    case KIND.role: {
      const role = ROLES[r.u8() ?? -1];
      const canShare = r.u8();
      const version = r.u64();
      return role && (canShare === 0 || canShare === 1) && version !== null && r.done()
        ? { t: 'role', role, canShare: canShare === 1, version }
        : null;
    }
    case KIND.subscribed: {
      const doc = r.doc();
      const seq = r.u64();
      if (doc === null || seq === null) return null;
      const version = r.rest();
      return parseVersion(version) ? { t: 'subscribed', doc, seq, version } : null;
    }
    case KIND.updates: {
      const doc = r.doc();
      const seq = r.u64();
      if (doc === null || seq === null) return null;
      const bytes = r.rest();
      // Пустая пачка — не пачка.
      return bytes.length > 0 && bytes.length <= MAX_SYNC_UPDATE_BYTES ? { t: 'updates', doc, seq, bytes } : null;
    }
    case KIND['snapshot-needed']: {
      const doc = r.doc();
      const url = r.str(1024);
      // Адрес — только снимок этого же документа на своём сервере: без схемы, `//` и `..`.
      return doc !== null && url !== null && r.done() && SNAPSHOT_RE.exec(url)?.[1] === doc ? { t: 'snapshot-needed', doc, url } : null;
    }
    case KIND.ack: {
      const doc = r.doc();
      const clientSeq = r.u32();
      const seq = r.u64();
      return doc !== null && clientSeq !== null && seq !== null && r.done() ? { t: 'ack', doc, clientSeq, seq } : null;
    }
    case KIND.refused: {
      const doc = r.doc();
      const clientSeq = r.u32();
      const code = SYNC_REFUSALS[r.u8() ?? -1];
      return doc !== null && clientSeq !== null && code && r.done() ? { t: 'refused', doc, clientSeq, code } : null;
    }
    case KIND.pause: {
      const ms = r.u32();
      return ms !== null && r.done() ? { t: 'pause', ms } : null;
    }
    case KIND['access-revoked']: {
      const reason = REASONS[r.u8() ?? -1];
      return reason && r.done() ? { t: 'access-revoked', reason } : null;
    }
    case KIND['please-reload']:
      return r.done() ? { t: 'please-reload' } : null;
    case KIND.pong:
      return r.done() ? { t: 'pong' } : null;
    case KIND['presence-batch']: {
      const items = parsePresenceBatch(b, 1);
      return items ? { t: 'presence', items } : null;
    }
    default:
      return null;
  }
}

/** Вектор версий на проводе: `n: u32 · (peer: u64 · counter: u32) × n`, как `SyncVersion` в ядре. Пир — десятичной
 * строкой: u64 не влезает в число JavaScript, а в JSON базы — так же. Сверх предела и с хвостом — `null`. Запись одна:
 * пиры строго по возрастанию, счётчик больше нуля, как кодирует ядро (`SyncVersion::encode`) и `encodeVersion`. */
export type VersionMap = Map<string, number>;

/** Вектор закодирован верно — без разбора в `Map`: сервер проверяет кадр подписки, а сам вектор читает потом, только
 * нужных пиров. Разобранный вектор на 65 536 пиров в восемь раз больше кадра. */
function validVersion(b: Uint8Array): boolean {
  if (b.length < 4) return false;
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const n = v.getUint32(0, true);
  if (n > MAX_VERSION_PEERS || b.length !== 4 + n * 12) return false;
  let prev = -1n;
  for (let i = 0; i < n; i++) {
    const at = 4 + i * 12;
    const peer = v.getBigUint64(at, true);
    const counter = v.getUint32(at + 8, true);
    if (counter === 0 || counter > 0x7fffffff || peer <= prev) return false;
    prev = peer;
  }
  return true;
}

export function parseVersion(b: Uint8Array): VersionMap | null {
  if (b.length < 4) return null;
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const n = v.getUint32(0, true);
  if (n > MAX_VERSION_PEERS || b.length !== 4 + n * 12) return null;
  const out: VersionMap = new Map();
  let prev = -1n;
  for (let i = 0; i < n; i++) {
    const at = 4 + i * 12;
    const peer = v.getBigUint64(at, true);
    const counter = v.getUint32(at + 8, true);
    if (counter === 0 || counter > 0x7fffffff || peer <= prev) return null;
    prev = peer;
    out.set(peer.toString(), counter);
  }
  return out;
}

/** Пустые записи (счётчик 0) ничего не значат и не кодируются: запись у одной версии одна. */
export function encodeVersion(m: VersionMap): Uint8Array<ArrayBuffer> {
  const peers = [...m.entries()]
    .filter(([peer, counter]) => {
      if (!/^(0|[1-9]\d{0,19})$/.test(peer) || BigInt(peer) > MAX_PEER) throw new RangeError(`пир: ${peer}`);
      checkInt(counter, 0x7fffffff, 'счётчик');
      return counter > 0;
    })
    .sort(([a], [b]) => {
      const [x, y] = [BigInt(a), BigInt(b)];
      return x < y ? -1 : x > y ? 1 : 0;
    });
  const out = new Uint8Array(4 + peers.length * 12);
  const v = new DataView(out.buffer);
  v.setUint32(0, peers.length, true);
  peers.forEach(([peer, counter], i) => {
    v.setBigUint64(4 + i * 12, BigInt(peer), true);
    v.setUint32(12 + i * 12, counter, true);
  });
  return out;
}

/** Содержит ли `a` всё из `b`. */
export function versionIncludes(a: VersionMap, b: VersionMap): boolean {
  for (const [peer, counter] of b) {
    if ((a.get(peer) ?? 0) < counter) return false;
  }
  return true;
}
