// Присутствие участников файла (спека подсистемы 4, раздел 9): сервер подписывает кадр участником своего соединения, держит
// последнее состояние каждого (для пришедших позже) и рассылает пачками по таймеру.
//
// Чего здесь нет намеренно: очереди кадров. У каждого получателя — только отметки «что у кого изменилось»; на таймере в пачку
// идёт последнее состояние, а не каждый кадр. Медленный читатель (буфер сокета выше `lowWaterBytes`) пропускает тики, его
// отметки сливаются, а память не растёт: работа сервера на получателя — не больше пачки за тик, сколько бы кадров ни
// присылали другие. Лишнее сверх пределов частоты молча отбрасывается: присутствие не рвёт соединение.
//
// Тело кадра разбирается только после пределов частоты: отброшенный кадр стоит проверки длины и бакета, а не разбора.
// Соединений с присутствием в файле и у человека в файле — не больше пределов: работа рассылки растёт как участники ×
// получатели, и без них читатели с десятками соединений роняли бы весь сервер. Сверх — соединение работает с документом, но
// его присутствие не принимается и ему не рассылается (молча, как прочие пределы); освободилось место — оно входит первым.

import {
  checkPartBody,
  encodePresenceBatch,
  encodePresenceRecord,
  parsePresenceBody,
  presenceColor,
  PRESENCE_BATCH_MAX,
  PRESENCE_FRAME_MAX,
  PRESENCE_NAME_MAX,
  ROLE_RANK,
  stripInvisible,
  withoutGesture,
  type FileRole,
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
  /** Соединений с присутствием в одном файле: спека рисует 50 участников (раздел 13.1), запас вдвое. */
  roomConnections: number;
  /** Соединений с присутствием у одного человека в одном файле: вкладок больше почти не бывает. */
  userConnections: number;
}

export const PRESENCE_LIMITS: PresenceLimits = {
  framesPerSecond: 30,
  framesBurst: 60,
  userFramesPerSecond: 120,
  userFramesBurst: 240,
  tickMs: 33,
  staleMs: 30_000,
  lowWaterBytes: 64 * 1024,
  roomConnections: 100,
  userConnections: 8,
};

/** Подпись участника без имени (поведение): не часть почты — её видят и читатели по ссылке. */
export const PRESENCE_ANON_NAME = 'Участник';

/** Соединение глазами присутствия: что нужно знать о нём хабу. */
export interface PresenceConn {
  id: number;
  file: string;
  user: string;
  /** Роль сейчас (сверка ролей её меняет): жест — только с правом правки. */
  role: FileRole;
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
  /** Номер участника в файле: счётчик комнаты, а не номер соединения сервера. */
  pid: number;
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
  /** Номер, под которым соединение будет участником файла. */
  pid: number;
  /** Отметки по номерам участников. */
  pending: Map<number, Pending>;
  /** Участники, о чьём `join` получателю уже отправлено и `left` ещё нет: только о них нужен `left`. */
  known: Set<number>;
}

/** Ждущий места: последнее тело состояния держится без разбора — вошёл, и сразу виден другим, не дожидаясь своего кадра. */
interface Waiting {
  conn: PresenceConn;
  name: string;
  body: Uint8Array | null;
}

/** Заголовок записи `state` пачки: вид, участник, длина тела (`encodePresenceRecord`). */
const STATE_RECORD_HEAD = 1 + 4 + 2;

interface Room {
  /** По номеру участника. */
  participants: Map<number, Participant>;
  /** По номеру соединения. */
  recipients: Map<number, Recipient>;
  /** Сверх пределов соединений: ждут места по порядку прихода. */
  waiting: Map<number, Waiting>;
  /** Соединений с присутствием у человека в этом файле. */
  perUser: Map<string, number>;
  /** Последний выданный номер участника. Растёт: новейшее соединение человека — с бóльшим номером (полоса аватаров следует
   * за ним). */
  next: number;
}

export class PresenceHub {
  private readonly rooms = new Map<string, Room>();
  private readonly users = new Map<string, Bucket>();
  private readonly timer: Timer;
  private readonly limits: PresenceLimits;
  /** Сколько пачек ушло и сколько тиков получатель пропустил из-за буфера, сколько кадров отброшено и сколько тел разобрано —
   * для тестов и пробы. */
  sent = 0;
  deferred = 0;
  dropped = 0;
  parsed = 0;

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
    if (!r) this.rooms.set(file, (r = { participants: new Map(), recipients: new Map(), waiting: new Map(), perUser: new Map(), next: 0 }));
    return r;
  }

  /** Роль соединения подтверждена: оно получает присутствие файла, начиная со всех, кто уже виден. Сверх пределов файла и
   * человека — ждёт места. */
  admit(conn: PresenceConn): void {
    if (conn.closed) return;
    const room = this.room(conn.file);
    if (room.recipients.has(conn.id) || room.waiting.has(conn.id)) return;
    if (this.fits(room, conn)) this.enter(room, conn);
    else room.waiting.set(conn.id, { conn, name: '', body: null });
  }

  private fits(room: Room, conn: PresenceConn): boolean {
    return room.recipients.size < this.limits.roomConnections && (room.perUser.get(conn.user) ?? 0) < this.limits.userConnections;
  }

  private enter(room: Room, conn: PresenceConn): Recipient {
    const r: Recipient = { conn, pid: ++room.next, pending: new Map(), known: new Set() };
    room.recipients.set(conn.id, r);
    room.perUser.set(conn.user, (room.perUser.get(conn.user) ?? 0) + 1);
    for (const p of room.participants.values()) {
      if (p.shown) r.pending.set(p.pid, everything(p));
    }
    return r;
  }

  /** Состояние участника. `name` — имя человека с сервера, не из кадра. Неверное и сверх пределов — молча мимо; тело
   * разбирается только после пределов. */
  state(conn: PresenceConn, name: string, body: Uint8Array): void {
    const room = this.rooms.get(conn.file);
    const r = room?.recipients.get(conn.id);
    if (room && !r && body.length < PRESENCE_FRAME_MAX) {
      // Ждущий: не рассылается, но последнее тело (в пределе частоты человека) держится до входа.
      const w = room.waiting.get(conn.id);
      if (w && this.allow(conn, undefined)) {
        w.name = name;
        w.body = body.slice();
      }
    }
    if (!room || !r || body.length >= PRESENCE_FRAME_MAX || !this.allow(conn, room.participants.get(r.pid))) {
      this.dropped++;
      return;
    }
    this.accept(room, r, name, body);
  }

  /** Состояние принято пределами: разбор и отметки получателям. */
  private accept(room: Room, r: Recipient, name: string, body: Uint8Array): void {
    const conn = r.conn;
    this.parsed++;
    const state = parsePresenceBody(body);
    if (!state) {
      this.dropped++;
      return;
    }
    const now = this.now();
    let p = room.participants.get(r.pid);
    if (!p) {
      p = {
        conn,
        pid: r.pid,
        join: encodePresenceRecord({
          k: 'join',
          participant: r.pid,
          user: conn.user,
          name: clip(name) || PRESENCE_ANON_NAME,
          color: presenceColor(conn.user),
        }),
        state: null,
        rev: -1,
        parts: [],
        last: now,
        shown: false,
        frames: new Bucket(this.limits.framesPerSecond, this.limits.framesBurst, now),
      };
      room.participants.set(r.pid, p);
    }
    p.last = now;
    // Жест — живое превью правки: без права правки его нет (как в Figma, читатель не тянет узлы), курсор и выделение остаются.
    const own = state.gesture && ROLE_RANK[conn.role] < ROLE_RANK.editor ? withoutGesture(body) : body.slice();
    p.state = encodePresenceRecord({ k: 'state', participant: r.pid, body: own });
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
    for (const x of room.recipients.values()) {
      if (x === r) continue;
      const q = pendingOf(x, r.pid);
      q.left = false;
      q.state = true;
      if (appearing) Object.assign(q, everything(p));
    }
  }

  /** Роль соединения сменилась (сверка ролей). Без права правки жест срезается и из сохранённого состояния — у других превью
   * снимается сразу, а не со следующим кадром соединения. */
  roleChanged(conn: PresenceConn): void {
    if (ROLE_RANK[conn.role] >= ROLE_RANK.editor) return;
    const room = this.rooms.get(conn.file);
    const r = room?.recipients.get(conn.id);
    const p = r && room?.participants.get(r.pid);
    if (!room || !r || !p?.state) return;
    const body = p.state.subarray(STATE_RECORD_HEAD);
    const own = withoutGesture(body);
    if (own.length === body.length) return;
    p.state = encodePresenceRecord({ k: 'state', participant: p.pid, body: own });
    if (!p.shown) return;
    for (const x of room.recipients.values()) if (x !== r) pendingOf(x, p.pid).state = true;
  }

  /** Часть выделения. Часть другой версии начинает её заново: кадры одного сокета идут по порядку, старых частей после новой
   * версии не бывает. */
  part(conn: PresenceConn, body: Uint8Array): void {
    const room = this.rooms.get(conn.file);
    const r = room?.recipients.get(conn.id);
    const p = r && room?.participants.get(r.pid);
    if (!room || !p || body.length >= PRESENCE_FRAME_MAX || !this.allow(conn, p)) {
      this.dropped++;
      return;
    }
    // Узлы проверяются без строк: сервер их не читает, а пересылает байты.
    this.parsed++;
    const part = checkPartBody(body);
    if (!part) {
      this.dropped++;
      return;
    }
    if (part.rev !== p.rev || part.parts !== p.parts.length) {
      p.rev = part.rev;
      p.parts = Array.from({ length: part.parts }, () => null);
    }
    p.parts[part.index] = encodePresenceRecord({ k: 'part', participant: p.pid, body: body.slice() });
    p.last = this.now();
    if (!p.shown) return;
    for (const x of room.recipients.values()) {
      if (x !== r) pendingOf(x, p.pid).parts.add(part.index);
    }
  }

  /** Пределы частоты: соединения и человека по всем соединениям. Сначала проверяются оба, токены берутся, только если
   * пропустят оба: отброшенный кадр не тратит токен другого предела. */
  private allow(conn: PresenceConn, p: Participant | undefined): boolean {
    const now = this.now();
    let u = this.users.get(conn.user);
    if (!u) this.users.set(conn.user, (u = new Bucket(this.limits.userFramesPerSecond, this.limits.userFramesBurst, now)));
    if ((p && !p.frames.has(now)) || !u.has(now)) return false;
    p?.frames.take(now);
    return u.take(now);
  }

  /** Соединение закрыто: участник пропадает у всех сразу, получатель уходит, его место занимает ждущий. */
  leave(conn: PresenceConn): void {
    const room = this.rooms.get(conn.file);
    if (!room) return;
    room.waiting.delete(conn.id);
    const r = room.recipients.get(conn.id);
    if (r) {
      room.recipients.delete(conn.id);
      const n = (room.perUser.get(conn.user) ?? 1) - 1;
      if (n > 0) room.perUser.set(conn.user, n);
      else room.perUser.delete(conn.user);
      const p = room.participants.get(r.pid);
      room.participants.delete(r.pid);
      if (p?.shown) this.hide(room, r.pid);
      for (const w of room.waiting.values()) {
        if (room.recipients.size >= this.limits.roomConnections) break;
        if (w.conn.closed || !this.fits(room, w.conn)) continue;
        room.waiting.delete(w.conn.id);
        const entered = this.enter(room, w.conn);
        // Его прежние кадры не рассылались: последнее состояние — сразу, клиент его не повторит до пульса.
        if (w.body) this.accept(room, entered, w.name, w.body);
      }
    }
    if (empty(room)) this.rooms.delete(conn.file);
  }

  private hide(room: Room, id: number): void {
    for (const r of room.recipients.values()) this.markLeft(r, id);
  }

  private markLeft(r: Recipient, id: number): void {
    // Получатель о нём не знает (его `join` ещё не ушёл): говорить об уходе нечего, отметка не копится.
    if (!r.known.has(id)) {
      r.pending.delete(id);
      return;
    }
    r.pending.set(id, { join: false, state: false, parts: new Set(), left: true });
  }

  /** Пачки получателям, у которых сокет не занят; молчащие дольше `staleMs` участники пропадают. */
  tick(): void {
    const now = this.now();
    for (const [file, room] of this.rooms) {
      for (const p of room.participants.values()) {
        if (p.shown && now - p.last > this.limits.staleMs) {
          p.shown = false;
          this.hide(room, p.pid);
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
      if (empty(room)) this.rooms.delete(file);
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
        r.known.delete(id);
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
        r.known.add(id);
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

  /** Сколько отметок «что отправить» ждёт самого загруженного получателя файла. Для тестов памяти. */
  pendingMax(file: string): number {
    let n = 0;
    for (const r of this.rooms.get(file)?.recipients.values() ?? []) n = Math.max(n, r.pending.size);
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

function empty(room: Room): boolean {
  return room.recipients.size === 0 && room.participants.size === 0 && room.waiting.size === 0;
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

/** Имя в кадре: без управляющих и невидимых символов (направление, нулевая ширина), не длиннее предела в байтах (по границе
 * символа). Имя из базы могло быть сохранено до чистки при вводе. */
export function clip(name: string): string {
  let s = [...stripInvisible(name)]
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
