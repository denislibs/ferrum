// Присутствие участников файла (спека подсистемы 4, раздел 9): сервер подписывает кадр участником своего соединения, держит
// последнее состояние каждого (для пришедших позже) и рассылает пачками по таймеру.
//
// Чего здесь нет намеренно: очереди кадров. У каждого получателя — только отметки «что у кого изменилось»; на таймере в пачку
// идёт последнее состояние, а не каждый кадр. Медленный читатель (буфер сокета выше `lowWaterBytes`) пропускает тики, его
// отметки сливаются, а память не растёт: работа сервера на получателя — не больше пачки за тик, сколько бы кадров ни
// присылали другие. Лишнее сверх пределов частоты молча отбрасывается: присутствие не рвёт соединение.

import {
  encodePresenceBatch,
  encodePresenceRecord,
  presenceColor,
  PRESENCE_BATCH_MAX,
  PRESENCE_NAME_MAX,
  type PresencePart,
  type PresenceState,
} from '@ferrum/protocol';

export interface PresenceLimits {
  /** Кадров присутствия на соединение: клиент шлёт не больше 20 в секунду, части выделения — сверх того. */
  framesPerSecond: number;
  framesBurst: number;
  /** На человека по всем вкладкам и соединениям. */
  userFramesPerSecond: number;
  userFramesBurst: number;
  /** Пачка каждому получателю не чаще. */
  tickMs: number;
  /** Без кадров дольше — участник пропадает у других (спека, раздел 9). */
  staleMs: number;
  /** Буфер сокета получателя выше этого — пачка ждёт следующего тика; правки документа идут своим путём до `highWaterBytes`. */
  lowWaterBytes: number;
}

export const PRESENCE_LIMITS: PresenceLimits = {
  framesPerSecond: 30,
  framesBurst: 60,
  userFramesPerSecond: 120,
  userFramesBurst: 240,
  tickMs: 33,
  staleMs: 30_000,
  lowWaterBytes: 64 * 1024,
};

/** Соединение глазами присутствия: что нужно знать о нём хабу. */
export interface PresenceConn {
  id: number;
  file: string;
  user: string;
  ws: { getBufferedAmount(): number; sendBinary(b: Uint8Array): number } | null;
  closed: boolean;
}

class Bucket {
  private tokens: number;
  private last: number;
  constructor(
    private readonly rate: number,
    private readonly burst: number,
    now: number,
  ) {
    this.tokens = burst;
    this.last = now;
  }
  has(now: number): boolean {
    return Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate) >= 1;
  }
  take(now: number): boolean {
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate);
    this.last = now;
    if (this.tokens < 1) return false;
    this.tokens -= 1;
    return true;
  }
  full(now: number): boolean {
    return Math.min(this.burst, this.tokens + ((now - this.last) / 1000) * this.rate) >= this.burst;
  }
}

/** Участник: последнее состояние и части выделения — уже закодированные записи пачки. */
interface Participant {
  conn: PresenceConn;
  join: Uint8Array;
  state: Uint8Array | null;
  rev: number;
  parts: (Uint8Array | null)[];
  last: number;
  /** Виден другим: прислал состояние и не молчал дольше `staleMs`. */
  shown: boolean;
  frames: Bucket;
}

/** Что получатель ещё не получил о каждом участнике. */
interface Pending {
  join: boolean;
  state: boolean;
  parts: Set<number>;
  left: boolean;
}

interface Recipient {
  conn: PresenceConn;
  pending: Map<number, Pending>;
}

interface Room {
  participants: Map<number, Participant>;
  recipients: Map<number, Recipient>;
}

export class PresenceHub {
  private readonly rooms = new Map<string, Room>();
  private readonly users = new Map<string, Bucket>();
  private readonly timer: Timer;
  private readonly limits: PresenceLimits;
  /** Сколько пачек ушло и сколько тиков получатель пропустил из-за буфера — для тестов и пробы. */
  sent = 0;
  deferred = 0;
  dropped = 0;

  constructor(
    limits: Partial<PresenceLimits> = {},
    private readonly now: () => number = () => performance.now(),
  ) {
    this.limits = { ...PRESENCE_LIMITS, ...limits };
    this.timer = setInterval(() => {
      try {
        this.tick();
      } catch (e) {
        console.error('Присутствие:', e instanceof Error ? e.message : e);
      }
    }, this.limits.tickMs);
    this.timer.unref?.();
  }

  private room(file: string): Room {
    let r = this.rooms.get(file);
    if (!r) this.rooms.set(file, (r = { participants: new Map(), recipients: new Map() }));
    return r;
  }

  /** Роль соединения подтверждена: оно получает присутствие файла, начиная со всех, кто уже виден. */
  admit(conn: PresenceConn): void {
    if (conn.closed) return;
    const room = this.room(conn.file);
    if (room.recipients.has(conn.id)) return;
    const r: Recipient = { conn, pending: new Map() };
    room.recipients.set(conn.id, r);
    for (const p of room.participants.values()) {
      if (p.shown && p.conn !== conn) r.pending.set(p.conn.id, everything(p));
    }
  }

  /** Состояние участника. `name` — имя человека с сервера, не из кадра. Неверное и сверх пределов — молча мимо. */
  state(conn: PresenceConn, name: string, body: Uint8Array, state: PresenceState | null | undefined): void {
    const room = this.rooms.get(conn.file);
    if (!state || !room?.recipients.has(conn.id) || !this.allow(conn)) {
      this.dropped++;
      return;
    }
    const now = this.now();
    let p = room.participants.get(conn.id);
    if (!p) {
      p = {
        conn,
        join: encodePresenceRecord({
          k: 'join',
          participant: conn.id,
          user: conn.user,
          name: clip(name),
          color: presenceColor(conn.user),
        }),
        state: null,
        rev: -1,
        parts: [],
        last: now,
        shown: false,
        frames: new Bucket(this.limits.framesPerSecond, this.limits.framesBurst, now),
      };
      room.participants.set(conn.id, p);
    }
    p.last = now;
    p.state = encodePresenceRecord({ k: 'state', participant: conn.id, body: body.slice() });
    const sel = state.selection;
    if (!sel) {
      p.rev = -1;
      p.parts = [];
    } else if (sel.rev !== p.rev || sel.parts !== p.parts.length) {
      p.rev = sel.rev;
      p.parts = Array.from({ length: sel.parts }, () => null);
    }
    const appearing = !p.shown;
    p.shown = true;
    for (const r of room.recipients.values()) {
      if (r.conn === conn) continue;
      const q = pendingOf(r, conn.id);
      q.left = false;
      q.state = true;
      if (appearing) Object.assign(q, everything(p));
    }
  }

  /** Часть выделения. Часть другой версии начинает её заново: кадры одного сокета идут по порядку, старых частей после новой
   * версии не бывает. */
  part(conn: PresenceConn, body: Uint8Array, part: PresencePart | null | undefined): void {
    const room = this.rooms.get(conn.file);
    const p = room?.participants.get(conn.id);
    if (!part || !room || !p || !this.allow(conn)) {
      this.dropped++;
      return;
    }
    if (part.rev !== p.rev || part.parts !== p.parts.length) {
      p.rev = part.rev;
      p.parts = Array.from({ length: part.parts }, () => null);
    }
    p.parts[part.index] = encodePresenceRecord({ k: 'part', participant: conn.id, body: body.slice() });
    p.last = this.now();
    if (!p.shown) return;
    for (const r of room.recipients.values()) {
      if (r.conn !== conn) pendingOf(r, conn.id).parts.add(part.index);
    }
  }

  /** Пределы частоты: соединения и человека по всем соединениям. Сначала проверяются оба, токены берутся, только если
   * пропустят оба: отброшенный кадр не тратит токен другого предела. */
  private allow(conn: PresenceConn): boolean {
    const now = this.now();
    const p = this.rooms.get(conn.file)?.participants.get(conn.id);
    let u = this.users.get(conn.user);
    if (!u) this.users.set(conn.user, (u = new Bucket(this.limits.userFramesPerSecond, this.limits.userFramesBurst, now)));
    if ((p && !p.frames.has(now)) || !u.has(now)) return false;
    p?.frames.take(now);
    return u.take(now);
  }

  /** Соединение закрыто: участник пропадает у всех сразу, получатель уходит. */
  leave(conn: PresenceConn): void {
    const room = this.rooms.get(conn.file);
    if (!room) return;
    room.recipients.delete(conn.id);
    const p = room.participants.get(conn.id);
    room.participants.delete(conn.id);
    if (p?.shown) this.hide(room, conn.id);
    if (room.recipients.size === 0 && room.participants.size === 0) this.rooms.delete(conn.file);
  }

  private hide(room: Room, id: number): void {
    for (const r of room.recipients.values()) this.markLeft(r, id);
  }

  private markLeft(r: Recipient, id: number): void {
    r.pending.set(id, { join: false, state: false, parts: new Set(), left: true });
  }

  /** Пачки получателям, у которых сокет не занят; молчащие дольше `staleMs` участники пропадают. */
  tick(): void {
    const now = this.now();
    for (const [file, room] of this.rooms) {
      for (const p of room.participants.values()) {
        if (p.shown && now - p.last > this.limits.staleMs) {
          p.shown = false;
          this.hide(room, p.conn.id);
        }
      }
      for (const r of room.recipients.values()) {
        if (r.pending.size === 0 || r.conn.closed || !r.conn.ws) continue;
        if (r.conn.ws.getBufferedAmount() > this.limits.lowWaterBytes) {
          this.deferred++;
          continue;
        }
        const records = this.collect(room, r);
        if (records.length === 0) continue;
        r.conn.ws.sendBinary(encodePresenceBatch(records));
        this.sent++;
      }
      if (room.recipients.size === 0 && room.participants.size === 0) this.rooms.delete(file);
    }
    for (const [user, b] of this.users) if (b.full(now)) this.users.delete(user);
  }

  /** Записи для получателя до предела пачки; что не влезло, остаётся отмеченным. */
  private collect(room: Room, r: Recipient): Uint8Array[] {
    const out: Uint8Array[] = [];
    let size = 0;
    const fits = (rec: Uint8Array) => {
      if (size + rec.length > PRESENCE_BATCH_MAX) return false;
      out.push(rec);
      size += rec.length;
      return true;
    };
    for (const [id, q] of r.pending) {
      if (q.left) {
        if (!fits(encodePresenceRecord({ k: 'left', participant: id }))) return out;
        r.pending.delete(id);
        continue;
      }
      const p = room.participants.get(id);
      if (!p?.shown) {
        r.pending.delete(id);
        continue;
      }
      if (q.join) {
        if (!fits(p.join)) return out;
        q.join = false;
      }
      if (q.state && p.state) {
        if (!fits(p.state)) return out;
        q.state = false;
      }
      for (const i of q.parts) {
        const rec = p.parts[i];
        if (rec && !fits(rec)) return out;
        q.parts.delete(i);
      }
      r.pending.delete(id);
    }
    return out;
  }

  /** Сколько байт держит присутствие: состояния, части и отметки. Для тестов памяти. */
  heldBytes(): number {
    let n = 0;
    for (const room of this.rooms.values()) {
      for (const p of room.participants.values()) {
        n += p.join.length + (p.state?.length ?? 0);
        for (const part of p.parts) n += part?.length ?? 0;
      }
      for (const r of room.recipients.values()) for (const q of r.pending.values()) n += 32 + q.parts.size * 8;
    }
    return n;
  }

  /** Сколько участников видно в файле. */
  shown(file: string): number {
    let n = 0;
    for (const p of this.rooms.get(file)?.participants.values() ?? []) if (p.shown) n++;
    return n;
  }

  stop(): void {
    clearInterval(this.timer);
  }
}

function pendingOf(r: Recipient, id: number): Pending {
  let q = r.pending.get(id);
  if (!q) r.pending.set(id, (q = { join: false, state: false, parts: new Set(), left: false }));
  return q;
}

function everything(p: Participant): Pending {
  return {
    join: true,
    state: p.state !== null,
    parts: new Set(p.parts.flatMap((part, i) => (part ? [i] : []))),
    left: false,
  };
}

/** Имя в кадре: без управляющих символов, не длиннее предела в байтах (по границе символа). */
export function clip(name: string): string {
  let s = [...name]
    .map((ch) => {
      const c = ch.codePointAt(0)!;
      return c < 0x20 || (c >= 0x7f && c <= 0x9f) || c === 0x2028 || c === 0x2029 ? ' ' : ch;
    })
    .join('')
    .trim();
  const enc = new TextEncoder();
  while (enc.encode(s).length > PRESENCE_NAME_MAX) s = [...s].slice(0, -1).join('');
  return s;
}
