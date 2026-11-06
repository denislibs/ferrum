// Присутствие (спека подсистемы 4, раздел 9): курсор, вид, выделение и живое превью жеста участника. Кадры — строгие двоичные,
// без `EphemeralStore` Loro: сервер должен прочесть каждое поле, чтобы подписать кадр своим участником и отбросить чужие
// поля, а разбор хранилища Loro на сервере стоит около 54 мкс на кадр против 0,3 мкс у своего формата (проба этапа 4).
//
// Числа — LE, координаты — f64 мира. Состояние кадра — целиком (идемпотентно): потерянный или выброшенный кадр
// заменяется следующим. Выделение длиннее одного кадра идёт частями своими кадрами с номером версии.

/** Предел кадра присутствия от клиента (спека, раздел 9): 2 КБ. */
export const PRESENCE_FRAME_MAX = 2048;
/** Координаты мира по модулю: дальше — не холст, а мусор. */
export const PRESENCE_WORLD_MAX = 1e9;
/** Выделение: столько узлов называет заголовок, частей — не больше `PRESENCE_PARTS_MAX`. Больше — только рамкой. */
export const PRESENCE_SELECTION_MAX = 100_000;
export const PRESENCE_PARTS_MAX = 16;
/** Байты узлов в одной части: остальное в кадре — заголовок части. */
export const PRESENCE_PART_BYTES = PRESENCE_FRAME_MAX - 8;
/** Пачка присутствия от сервера одному получателю: остальное уйдёт следующей. */
export const PRESENCE_BATCH_MAX = 64 * 1024;
/** Имя участника в кадре, байты UTF-8, и `userId`. */
export const PRESENCE_NAME_MAX = 256;
export const PRESENCE_USER_MAX = 128;

/** Цвета участников (как в Figma — насыщенные, белый текст на них читается). Индекс — из хэша `userId`. */
export const PRESENCE_COLORS = [
  '#e5484d',
  '#f76b15',
  '#2d8a3e',
  '#0d74ce',
  '#8e4ec6',
  '#d6409f',
  '#12a594',
  '#5b5bd6',
  '#ab6400',
  '#3e63dd',
  '#ca244d',
  '#00749e',
] as const;

/** Цвет участника: FNV-1a от `userId`, один и тот же у всех получателей и во всех вкладках человека. */
export function presenceColor(user: string): number {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(user)) {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h % PRESENCE_COLORS.length;
}

/** Жест, который участник сейчас тянет: превью видно другим, в документ не идёт. */
export const GESTURE_KINDS = ['move', 'resize', 'rotate', 'scale'] as const;
export type GestureKind = (typeof GESTURE_KINDS)[number];

/** Матрица `[a, b, c, d, e, f]`, как `kurbo::Affine`. */
export type Affine6 = [number, number, number, number, number, number];
export type Rect4 = [number, number, number, number];

export interface PresenceSelection {
  /** Версия выделения: части с той же версией складываются вместе. */
  rev: number;
  count: number;
  parts: number;
  /** Общая рамка в мире: её видно, пока части не пришли, и у выделения больше частей. */
  bounds: Rect4;
}

export interface PresenceGesture {
  kind: GestureKind;
  /** Мировая матрица жеста (перенос, ресайз, поворот) и рамка выделения на начало жеста. */
  transform: Affine6;
  frame: Affine6;
  /** Равномерный коэффициент «Масштаба» (у остальных 1). */
  k: number;
}

export interface PresenceState {
  /** Страница — `TreeID` документа страницы. */
  page: string;
  /** Видимая часть мира. */
  view: Rect4;
  /** Курсор в мире; нет — указатель вне холста. */
  cursor: [number, number] | null;
  selection: PresenceSelection | null;
  gesture: PresenceGesture | null;
}

export interface PresencePart {
  rev: number;
  index: number;
  parts: number;
  /** Узлы, `counter@peer`, как `TreeID` Loro. */
  ids: string[];
}

/** Записи пачки сервера: кто пришёл, его состояние, часть выделения, кто ушёл. Участник — номер соединения. */
export type PresenceItem =
  | { k: 'join'; participant: number; user: string; name: string; color: number }
  | { k: 'state'; participant: number; state: PresenceState }
  | { k: 'part'; participant: number; part: PresencePart }
  | { k: 'left'; participant: number };

const utf8 = new TextEncoder();
const fromUtf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const PAGE_RE = /^(0|[1-9]\d{0,9})@(0|[1-9]\d{0,19})$/;
const MAX_PEER = 2n ** 64n - 1n;
const MAX_COUNTER = 0x7fffffff;

export function isPageDoc(s: string): boolean {
  if (!PAGE_RE.test(s)) return false;
  const [counter, peer] = s.split('@');
  return Number(counter) <= MAX_COUNTER && BigInt(peer) <= MAX_PEER;
}

const coord = (v: number) => Number.isFinite(v) && Math.abs(v) <= PRESENCE_WORLD_MAX;

/** Писатель с проверкой: неверное значение — ошибка здесь, а не неверный кадр на проводе. */
class Out {
  private readonly parts: Uint8Array[] = [];
  size = 0;
  push(p: Uint8Array): this {
    this.parts.push(p);
    this.size += p.length;
    return this;
  }
  u8(v: number): this {
    if (!Number.isInteger(v) || v < 0 || v > 0xff) throw new RangeError(`u8: ${v}`);
    return this.push(Uint8Array.of(v));
  }
  u16(v: number): this {
    if (!Number.isInteger(v) || v < 0 || v > 0xffff) throw new RangeError(`u16: ${v}`);
    const b = new Uint8Array(2);
    new DataView(b.buffer).setUint16(0, v, true);
    return this.push(b);
  }
  u32(v: number): this {
    if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) throw new RangeError(`u32: ${v}`);
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, true);
    return this.push(b);
  }
  f64(v: number): this {
    if (!coord(v)) throw new RangeError(`число: ${v}`);
    const b = new Uint8Array(8);
    new DataView(b.buffer).setFloat64(0, v, true);
    return this.push(b);
  }
  str(s: string, max: number): this {
    const b = utf8.encode(s);
    if (b.length > max) throw new RangeError(`строка длиннее ${max} байт`);
    return this.u16(b.length).push(b);
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

class In {
  at: number;
  private readonly view: DataView;
  constructor(
    private readonly b: Uint8Array,
    at = 0,
    readonly end = b.length,
  ) {
    this.at = at;
    this.view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  }
  has(n: number): boolean {
    return this.at + n <= this.end;
  }
  u8(): number | null {
    return this.has(1) ? this.b[this.at++] : null;
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
  u64(): bigint | null {
    if (!this.has(8)) return null;
    const v = this.view.getBigUint64(this.at, true);
    this.at += 8;
    return v;
  }
  /** Конечное число в пределах мира; иначе `null`. */
  f64(): number | null {
    if (!this.has(8)) return null;
    const v = this.view.getFloat64(this.at, true);
    this.at += 8;
    return coord(v) ? v : null;
  }
  bytes(n: number): Uint8Array | null {
    if (!this.has(n)) return null;
    const v = this.b.subarray(this.at, this.at + n);
    this.at += n;
    return v;
  }
  str(max: number): string | null {
    const n = this.u16();
    if (n === null || n > max) return null;
    const raw = this.bytes(n);
    if (!raw) return null;
    try {
      return fromUtf8.decode(raw);
    } catch {
      return null;
    }
  }
  page(): string | null {
    const n = this.u8();
    if (n === null || n > 40) return null;
    const raw = this.bytes(n);
    if (!raw) return null;
    try {
      const s = fromUtf8.decode(raw);
      return isPageDoc(s) ? s : null;
    } catch {
      return null;
    }
  }
  /** LEB128 до u32, без лишних байтов: у числа одна запись. */
  varint(): number | null {
    let v = 0;
    for (let i = 0; i < 5; i++) {
      const b = this.u8();
      if (b === null) return null;
      v += (b & 0x7f) * 2 ** (7 * i);
      if ((b & 0x80) === 0) {
        if (i > 0 && b === 0) return null;
        return v <= 0xffffffff ? v : null;
      }
    }
    return null;
  }
  done(): boolean {
    return this.at === this.end;
  }
}

function varint(out: Out, v: number): void {
  if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) throw new RangeError(`varint: ${v}`);
  const bytes: number[] = [];
  do {
    let b = v % 128;
    v = Math.floor(v / 128);
    if (v > 0) b |= 0x80;
    bytes.push(b);
  } while (v > 0);
  out.push(Uint8Array.from(bytes));
}

const FLAG_CURSOR = 1;
const FLAG_SELECTION = 2;
const FLAG_GESTURE = 4;

function writeRect(out: Out, r: Rect4): void {
  for (const v of r) out.f64(v);
}

function readRect(r: In, strict: boolean): Rect4 | null {
  const x0 = r.f64();
  const y0 = r.f64();
  const x1 = r.f64();
  const y1 = r.f64();
  if (x0 === null || y0 === null || x1 === null || y1 === null) return null;
  if (strict ? x1 <= x0 || y1 <= y0 : x1 < x0 || y1 < y0) return null;
  return [x0, y0, x1, y1];
}

function readAffine(r: In): Affine6 | null {
  const out: number[] = [];
  for (let i = 0; i < 6; i++) {
    const v = r.f64();
    if (v === null) return null;
    out.push(v);
  }
  return out as Affine6;
}

/** Тело кадра состояния (без вида кадра). */
function writePresenceState(out: Out, s: PresenceState): void {
  if (!isPageDoc(s.page)) throw new RangeError(`страница: ${s.page}`);
  const flags = (s.cursor ? FLAG_CURSOR : 0) | (s.selection ? FLAG_SELECTION : 0) | (s.gesture ? FLAG_GESTURE : 0);
  out.u8(flags);
  const page = utf8.encode(s.page);
  out.u8(page.length).push(page);
  writeRect(out, s.view);
  if (s.cursor) out.f64(s.cursor[0]).f64(s.cursor[1]);
  if (s.selection) {
    const sel = s.selection;
    if (sel.count < 1 || sel.count > PRESENCE_SELECTION_MAX || sel.parts < 1 || sel.parts > PRESENCE_PARTS_MAX)
      throw new RangeError('выделение');
    out.u32(sel.rev).u32(sel.count).u8(sel.parts);
    writeRect(out, sel.bounds);
  }
  if (s.gesture) {
    out.u8(GESTURE_KINDS.indexOf(s.gesture.kind));
    for (const v of s.gesture.transform) out.f64(v);
    for (const v of s.gesture.frame) out.f64(v);
    out.f64(s.gesture.k);
  }
}

function readPresenceState(r: In): PresenceState | null {
  const flags = r.u8();
  if (flags === null || flags & ~(FLAG_CURSOR | FLAG_SELECTION | FLAG_GESTURE)) return null;
  const page = r.page();
  const view = readRect(r, true);
  if (page === null || view === null) return null;
  let cursor: [number, number] | null = null;
  if (flags & FLAG_CURSOR) {
    const x = r.f64();
    const y = r.f64();
    if (x === null || y === null) return null;
    cursor = [x, y];
  }
  let selection: PresenceSelection | null = null;
  if (flags & FLAG_SELECTION) {
    const rev = r.u32();
    const count = r.u32();
    const parts = r.u8();
    const bounds = readRect(r, false);
    if (rev === null || count === null || parts === null || bounds === null) return null;
    if (count < 1 || count > PRESENCE_SELECTION_MAX || parts < 1 || parts > PRESENCE_PARTS_MAX) return null;
    selection = { rev, count, parts, bounds };
  }
  let gesture: PresenceGesture | null = null;
  if (flags & FLAG_GESTURE) {
    const kind = GESTURE_KINDS[r.u8() ?? -1];
    const transform = readAffine(r);
    const frame = readAffine(r);
    const k = r.f64();
    if (!kind || transform === null || frame === null || k === null) return null;
    gesture = { kind, transform, frame, k };
  }
  return { page, view, cursor, selection, gesture };
}

/** Узлы части: группы по пиру (пиры по возрастанию), в группе — счётчики по возрастанию разностями. */
export function encodeSelectionIds(ids: string[]): Uint8Array<ArrayBuffer> {
  const byPeer = new Map<bigint, number[]>();
  for (const id of ids) {
    if (!isPageDoc(id)) throw new RangeError(`узел: ${id}`);
    const [c, p] = id.split('@');
    const peer = BigInt(p);
    let list = byPeer.get(peer);
    if (!list) byPeer.set(peer, (list = []));
    list.push(Number(c));
  }
  const out = new Out();
  for (const peer of [...byPeer.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))) {
    const counters = [...new Set(byPeer.get(peer)!)].sort((a, b) => a - b);
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, peer, true);
    out.push(b);
    varint(out, counters.length);
    varint(out, counters[0]);
    for (let i = 1; i < counters.length; i++) varint(out, counters[i] - counters[i - 1]);
  }
  return out.done();
}

/** Разбор узлов части. Запись одна: пиры строго по возрастанию, разности больше нуля, счётчик в пределах `TreeID`. */
export function decodeSelectionIds(b: Uint8Array): string[] | null {
  const r = new In(b);
  const out: string[] = [];
  let prev = -1n;
  while (!r.done()) {
    const peer = r.u64();
    const n = r.varint();
    if (peer === null || n === null || peer <= prev || n < 1) return null;
    prev = peer;
    // Каждый узел — хотя бы байт: заголовок не заставит разбирать больше, чем пришло.
    if (n > r.end - r.at) return null;
    let counter = r.varint();
    if (counter === null || counter > MAX_COUNTER) return null;
    out.push(`${counter}@${peer}`);
    for (let i = 1; i < n; i++) {
      const d = r.varint();
      if (d === null || d < 1) return null;
      counter += d;
      if (counter > MAX_COUNTER) return null;
      out.push(`${counter}@${peer}`);
    }
  }
  return out.length > 0 ? out : null;
}

/** Разбивает выделение на части, каждая — не больше `PRESENCE_PART_BYTES`. Сверх `PRESENCE_PARTS_MAX` — не уходит. */
export function splitSelection(ids: string[]): Uint8Array<ArrayBuffer>[] {
  const parts: Uint8Array<ArrayBuffer>[] = [];
  let at = 0;
  while (at < ids.length && parts.length < PRESENCE_PARTS_MAX) {
    // Кусок, который точно влезает (по 14 байт на узел в худшем случае), затем добор, пока влезает.
    let n = Math.min(ids.length - at, Math.floor(PRESENCE_PART_BYTES / 14));
    let bytes = encodeSelectionIds(ids.slice(at, at + n));
    for (let step = 64; step >= 1; step = Math.floor(step / 2)) {
      while (at + n + step <= ids.length) {
        const next = encodeSelectionIds(ids.slice(at, at + n + step));
        if (next.length > PRESENCE_PART_BYTES) break;
        n += step;
        bytes = next;
      }
    }
    parts.push(bytes);
    at += n;
  }
  return parts;
}

/** Кадр клиента: состояние (вид `kind`) — тело без вида кадра. */
export function encodePresenceBody(s: PresenceState): Uint8Array<ArrayBuffer> {
  const out = new Out();
  writePresenceState(out, s);
  return out.done();
}

export function encodePartBody(p: { rev: number; index: number; parts: number; bytes: Uint8Array }): Uint8Array<ArrayBuffer> {
  if (p.parts < 1 || p.parts > PRESENCE_PARTS_MAX || p.index >= p.parts) throw new RangeError('часть выделения');
  if (p.bytes.length < 1 || p.bytes.length > PRESENCE_PART_BYTES) throw new RangeError('байты части');
  return new Out().u32(p.rev).u8(p.index).u8(p.parts).push(p.bytes).done();
}

/** Тело состояния от клиента: разобрано строго, иначе `null`. Сервер пересылает эти же байты: лишнего в них нет. */
export function parsePresenceBody(b: Uint8Array): PresenceState | null {
  if (b.length > PRESENCE_FRAME_MAX) return null;
  const r = new In(b);
  const s = readPresenceState(r);
  return s && r.done() ? s : null;
}

export function parsePartBody(b: Uint8Array): PresencePart | null {
  if (b.length > PRESENCE_FRAME_MAX) return null;
  const r = new In(b);
  const rev = r.u32();
  const index = r.u8();
  const parts = r.u8();
  if (rev === null || index === null || parts === null || parts < 1 || parts > PRESENCE_PARTS_MAX || index >= parts) return null;
  const rest = r.bytes(r.end - r.at);
  if (!rest || rest.length > PRESENCE_PART_BYTES) return null;
  const ids = decodeSelectionIds(rest);
  return ids ? { rev, index, parts, ids } : null;
}

const ITEM = { join: 1, state: 2, part: 3, left: 4 } as const;

/** Запись пачки сервера. Состояние и часть — байты, уже проверенные разбором при приёме. */
export type PresenceRecord =
  | { k: 'join'; participant: number; user: string; name: string; color: number }
  | { k: 'state'; participant: number; body: Uint8Array }
  | { k: 'part'; participant: number; body: Uint8Array }
  | { k: 'left'; participant: number };

export function encodePresenceRecord(rec: PresenceRecord): Uint8Array<ArrayBuffer> {
  const out = new Out().u8(ITEM[rec.k]).u32(rec.participant);
  switch (rec.k) {
    case 'join':
      if (rec.color < 0 || rec.color >= PRESENCE_COLORS.length) throw new RangeError('цвет');
      return out.str(rec.user, PRESENCE_USER_MAX).str(rec.name, PRESENCE_NAME_MAX).u8(rec.color).done();
    case 'state':
    case 'part':
      return out.u16(rec.body.length).push(rec.body).done();
    case 'left':
      return out.done();
  }
}

/** Пачка сервера: записи подряд. Разбор у клиента строгий: сеть не доверенная, неверное — `null`. */
export function parsePresenceBatch(b: Uint8Array, at: number): PresenceItem[] | null {
  if (b.length - at > PRESENCE_BATCH_MAX) return null;
  const r = new In(b, at);
  const items: PresenceItem[] = [];
  while (!r.done()) {
    const kind = r.u8();
    const participant = r.u32();
    if (kind === null || participant === null) return null;
    switch (kind) {
      case ITEM.join: {
        const user = r.str(PRESENCE_USER_MAX);
        const name = r.str(PRESENCE_NAME_MAX);
        const color = r.u8();
        if (user === null || user.length === 0 || name === null || color === null || color >= PRESENCE_COLORS.length) return null;
        items.push({ k: 'join', participant, user, name, color });
        break;
      }
      case ITEM.state:
      case ITEM.part: {
        const n = r.u16();
        if (n === null) return null;
        const body = r.bytes(n);
        if (!body) return null;
        if (kind === ITEM.state) {
          const state = parsePresenceBody(body);
          if (!state) return null;
          items.push({ k: 'state', participant, state });
        } else {
          const part = parsePartBody(body);
          if (!part) return null;
          items.push({ k: 'part', participant, part });
        }
        break;
      }
      case ITEM.left:
        items.push({ k: 'left', participant });
        break;
      default:
        return null;
    }
  }
  return items.length > 0 ? items : null;
}
