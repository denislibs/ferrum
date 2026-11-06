// Файлы на сервере со стороны worker'а (спека подсистемы 4, раздел 6): запросы, разбор ответов и всё, что
// размером с документ. Основной поток сюда не заходит: байты приходят и уходят передачей. Сеть не
// доверенная: неожиданный вид ответа — `NETWORK`, а не `undefined` в интерфейсе; у каждого запроса срок.

import {
  API_ERROR_CODES,
  API_FILES,
  ContainerError,
  containerHeader,
  imageSectionHeader,
  isFileAccess,
  isFileList,
  isFileMeta,
  isHash,
  LOST_IMAGES_HEADER,
  MAX_IMAGE_REFS,
  parseSection,
  PREVIOUS_WRITE_HEADER,
  scanContainer,
  SECTION_IMAGE,
  SECTION_PAGE_REFS,
  USER_HEADER,
  VERSION_HEADER,
  WRITE_ID_HEADER,
  type ApiErrorCode,
  type FileAccessDto,
  type FileMetaDto,
  type FileRole,
  type SectionSpan,
} from '@ferrum/protocol';
import { MAX_THUMB_BYTES, SERVER_CONFLICT, type FileMeta, type IoResults } from './file-list';
import { MAX_OPEN_BYTES, tooLarge } from './limits';

export type Failure = ApiErrorCode | 'NETWORK';

export class ServerFailure extends Error {
  constructor(
    readonly code: Failure,
    message = failureMessage(code),
  ) {
    super(message);
  }
}

export function failureMessage(code: Failure): string {
  switch (code) {
    case 'CONFLICT':
      return SERVER_CONFLICT;
    case 'NOT_FOUND':
      return 'Нет доступа к файлу или он удалён';
    case 'FORBIDDEN':
      return 'Недостаточно прав';
    case 'UNAUTHORIZED':
      return 'Сессия завершена — войдите снова';
    case 'TOO_LARGE':
      return tooLarge(MAX_OPEN_BYTES);
    case 'BAD_CONTAINER':
      return 'Это не файл Ferrum';
    case 'TOO_MANY_REQUESTS':
    case 'BUSY':
      return 'Слишком много запросов — повторите позже';
    case 'UNAVAILABLE':
      return 'Сервер недоступен — повторите позже';
    case 'INCOMPLETE':
      return 'Копию не собрать целиком: часть файла на сервере не сжимается. Правки целы — откройте файл и сохраните копию из редактора';
    case 'MISSING_IMAGES':
      return 'Картинки файла не дошли до сервера — повторите позже';
    case 'TOO_MANY_IMAGES':
      return `В файле больше ${MAX_IMAGE_REFS.toLocaleString('ru-RU')} картинок — такой файл на сервере не хранится. Правки останутся в этом браузере`;
    case 'NETWORK':
      return 'Нет связи с сервером';
    default:
      return 'Сервер отклонил запрос';
  }
}

const isErrorCode = (v: unknown): v is ApiErrorCode => (API_ERROR_CODES as readonly unknown[]).includes(v);

/** Дольше этого не ждём небольшой запрос: то же, что и у входа. */
export const JSON_TIMEOUT_MS = 15_000;
/** Опрос открытого файла: снятие доступа и понижение роли видны не позже этого. */
export const ACCESS_POLL_MS = 15_000;
/** Превью — не чаще раза в минуту (спека, раздел 6.2). */
export const THUMB_INTERVAL_MS = 60_000;
/** Срок передачи по размеру: от 64 КиБ/с, но не меньше минуты. Верхнего предела нет: 512 МБ на канале в 1 Мбит/с
 * идут час, а застывшее соединение и так оборвёт сервер (простой минута, 32 КиБ/с в среднем, `UPLOAD_PACE`). */
const MIN_TRANSFER_MS = 60_000;
export const transferTimeout = (bytes: number): number => Math.ceil(MIN_TRANSFER_MS + (bytes / (64 * 1024)) * 1000);
/** Картинок загружается одновременно. */
const IMAGE_CONCURRENCY = 4;
/** Запрос картинки под пределом частоты: столько попыток с паузой, какую просит сервер. Окно предела минута,
 * поэтому файл на пределе `MAX_IMAGE_REFS` проходит с запасом, а заставка не вечна. */
const RATE_ATTEMPTS = 6;
const RATE_DEFAULT_WAIT_SECONDS = 5;
const RATE_MAX_WAIT_SECONDS = 60;

/** Запросы вкладки идут от того, кто в ней вошёл: сервер сверяет это с сессией (`USER_HEADER`). */
export function asUser(user: string, http: typeof fetch): typeof fetch {
  return (input, init = {}) => {
    const headers = new Headers(init.headers);
    headers.set(USER_HEADER, user);
    return http(input, { ...init, headers });
  };
}

/** `canShare`: сервер разрешает этому человеку звать других (ответа без поля — считаем, что нет).
 * `own`: идёт своя запись или её исход неизвестен, поэтому новое поколение может быть своим. */
export type AccessState =
  { kind: 'role'; role: FileRole; version: number; canShare: boolean; own: boolean } | { kind: 'none' } | { kind: 'session' };

export interface JournalEntries {
  entries: ArrayBuffer[];
  session: number | null;
  tooLarge: boolean;
  /** Ключи `entries`: снять именно их, когда сервер подтвердил. */
  keys?: JournalKey[];
}

export type JournalKey = [file: string, session: number, batch: number];

/** Журнал правок этого человека (в браузере — IndexedDB с разделением по пользователю). */
export interface JournalStore {
  append(file: string, session: number, batch: number, bytes: ArrayBuffer): Promise<void>;
  read(file: string): Promise<JournalEntries>;
  /** Снимок вобрал пачки до `upTo` этого сеанса и все прежние. */
  cut(file: string, session: number, upTo: number): Promise<void>;
  /** Только эти пачки: у журнала сервера файл открыт в нескольких вкладках, пачки других вкладок ещё нужны. */
  cutExact(keys: readonly JournalKey[]): Promise<void>;
  drop(file: string): Promise<void>;
}

export interface ServerEvents {
  changed(): void;
  access(file: string, state: AccessState): void;
}

export interface Clock {
  now(): number;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(t: unknown): void;
}

function toMeta(d: FileMetaDto): FileMeta {
  return { ...d };
}

function record(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

/** Куда файл читается: из памяти (снимок ядра) или из `Blob` (файл с диска, не загружается целиком). */
interface Source {
  size: number;
  read(start: number, end: number): Promise<Uint8Array>;
  part(start: number, end: number): BlobPart;
}

function bytesSource(bytes: Uint8Array<ArrayBuffer>): Source {
  return {
    size: bytes.length,
    read: async (s, e) => bytes.subarray(s, e),
    part: (s, e) => bytes.subarray(s, e),
  };
}

function blobSource(blob: Blob): Source {
  return {
    size: blob.size,
    read: async (s, e) => new Uint8Array(await blob.slice(s, e).arrayBuffer()),
    part: (s, e) => blob.slice(s, e),
  };
}

interface Split {
  /** Файл без картинок: то, что хранит сервер (раздел 7.5). */
  body: BodyInit;
  size: number;
  images: { hash: string; size: number; part: BlobPart }[];
  /** Ссылок содержимого, для которых в файле нет байтов: потеряны раньше, загрузить нечего. */
  lost: number;
  /** Разных картинок, на которые ссылается содержимое. */
  refs: number;
}

/** Хэши картинок, на которые ссылаются страницы. Секции ссылок небольшие. */
async function imageRefs(source: Pick<Source, 'read'>, spans: SectionSpan[]): Promise<Set<string>> {
  const hashes = new Set<string>();
  for (const s of spans) {
    if (s.kind !== SECTION_PAGE_REFS) continue;
    for (const line of new TextDecoder().decode(await source.read(s.data, s.data + s.size)).split('\n')) {
      if (isHash(line)) hashes.add(line);
    }
  }
  return hashes;
}

/** Картинки уходят отдельными запросами, остальное — как есть, без копии, если картинок нет. */
async function splitContainer(source: Source, whole: () => BodyInit): Promise<Split> {
  let spans: SectionSpan[];
  try {
    spans = await scanContainer(source.size, source.read);
  } catch (e) {
    if (e instanceof ContainerError) throw new ServerFailure('BAD_CONTAINER');
    throw e;
  }
  const images = spans.filter((s) => s.kind === SECTION_IMAGE && isHash(s.id));
  const present = new Set(images.map((s) => s.id));
  const refs = await imageRefs(source, spans);
  const lost = [...refs].filter((h) => !present.has(h)).length;
  if (images.length === 0 && spans.every((s) => s.kind !== SECTION_IMAGE)) {
    return { body: whole(), size: source.size, images: [], lost, refs: refs.size };
  }
  const kept = spans.filter((s) => s.kind !== SECTION_IMAGE);
  const parts: BlobPart[] = [containerHeader(kept.length) as Uint8Array<ArrayBuffer>];
  let size = 16;
  for (const s of kept) {
    parts.push(source.part(s.start, s.data + s.size));
    size += s.data + s.size - s.start;
  }
  return {
    body: new Blob(parts),
    size,
    images: images.map((s) => ({ hash: s.id, size: s.size, part: source.part(s.data, s.data + s.size) })),
    lost,
    refs: refs.size,
  };
}

async function inPool<T>(items: T[], limit: number, work: (item: T) => Promise<void>): Promise<void> {
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (next < items.length) await work(items[next++]);
    }),
  );
}

export class ServerFiles {
  /** Записи с неизвестным исходом (сеть оборвалась), по порядку: каждая основана на предыдущей, если та
   * дошла. Следующая несёт номер той, что сделала нынешнее поколение, и своя удачная запись не станет
   * `CONFLICT`. Решает сервер: он сверяет номер с последней записью файла. */
  private readonly uncertain = new Map<string, string[]>();
  /** Идущие и начатые записи файла: ответ опроса, заставший запись, может показывать своё поколение. */
  private readonly writes = new Map<string, { active: number; begun: number }>();
  /** Журнал не применён (файл открыт читателем): запись его не стирает, он дождётся открытия. */
  private readonly heldJournal = new Set<string>();
  private accessSeq = 0;
  /** Последний учтённый запрос доступа: опоздавший ответ не отменяет более новый. */
  private readonly accessAnswered = new Map<string, number>();
  private readonly knownImages = new Map<string, Set<string>>();
  private readonly thumbs = new Map<string, { last: number; pending: ArrayBuffer | null | undefined; timer: unknown }>();
  private readonly lastAccess = new Map<string, string>();
  /** Пачки журнала, прочитанные при открытии файла: они вошли в документ, их снимает подтверждение сервера. */
  private readonly readKeys = new Map<string, readonly JournalKey[]>();

  constructor(
    private readonly http: typeof fetch,
    private readonly journal: JournalStore,
    private readonly events: ServerEvents,
    private readonly clock: Clock,
    private readonly newId: () => string = () => crypto.randomUUID(),
  ) {}

  /** Картинок в файле бывает много, а предел частоты общий на человека: 429 — не сбой, а просьба подождать. */
  private async paced(send: () => Promise<Response>): Promise<Response> {
    for (let attempt = 1; ; attempt++) {
      const res = await send();
      if (res.status !== 429 || attempt === RATE_ATTEMPTS) return res;
      void res.body?.cancel().catch(() => {});
      const asked = Number(res.headers.get('retry-after'));
      const wait = Number.isFinite(asked) && asked > 0 ? Math.min(asked, RATE_MAX_WAIT_SECONDS) : RATE_DEFAULT_WAIT_SECONDS;
      await new Promise<void>((resolve) => void this.clock.setTimeout(resolve, wait * 1000));
    }
  }

  /** Запрос с небольшим телом и ответом. Повтор один раз — только у чтения и только при сбое сети или
   * 5xx: запись повторяет тот, кто знает, можно ли. */
  private async json<T>(path: string, valid: (v: unknown) => v is T, init: RequestInit = {}): Promise<T> {
    const method = init.method ?? 'GET';
    const attempt = async (): Promise<T> => {
      const signal = AbortSignal.timeout(JSON_TIMEOUT_MS);
      let res: Response;
      try {
        res = await this.http(path, { ...init, signal });
      } catch {
        throw new ServerFailure('NETWORK');
      }
      return this.readReply(res, valid);
    };
    try {
      return await attempt();
    } catch (e) {
      const retry = method === 'GET' && e instanceof ServerFailure && (e.code === 'NETWORK' || e.code === 'UNAVAILABLE');
      if (!retry) throw e;
      return attempt();
    }
  }

  private async readReply<T>(res: Response, valid: (v: unknown) => v is T): Promise<T> {
    if (res.status >= 500) throw new ServerFailure('UNAVAILABLE');
    let body: unknown;
    try {
      body = await res.json();
    } catch {
      throw new ServerFailure('NETWORK');
    }
    if (res.ok) {
      if (valid(body)) return body;
      throw new ServerFailure('NETWORK');
    }
    const code = record(body)?.error;
    throw new ServerFailure(isErrorCode(code) ? code : 'NETWORK');
  }

  private post<T>(path: string, valid: (v: unknown) => v is T, body?: unknown): Promise<T> {
    return this.json(path, valid, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  }

  private note(file: string, failure: unknown): void {
    if (!(failure instanceof ServerFailure)) return;
    if (failure.code === 'NOT_FOUND') this.announce(file, { kind: 'none' });
    else if (failure.code === 'UNAUTHORIZED') this.announce(file, { kind: 'session' });
    else if (failure.code === 'FORBIDDEN') void this.checkAccess(file).catch(() => {});
  }

  /** Состояние доступа открытому редактору: только когда оно изменилось. */
  private announce(file: string, state: AccessState): void {
    const key = JSON.stringify(state);
    if (this.lastAccess.get(file) === key) return;
    this.lastAccess.set(file, key);
    this.events.access(file, state);
  }

  /** Лёгкая проверка: сеть оборвалась — ничего не меняется, это забота следующей попытки. Ответы
   * приходят в любом порядке: старый не возвращает прежнюю роль, а снятое наблюдение не оживает. */
  async checkAccess(file: string): Promise<void> {
    const my = ++this.accessSeq;
    const fresh = () => {
      if (my < (this.accessAnswered.get(file) ?? 0)) return false;
      this.accessAnswered.set(file, my);
      return true;
    };
    const before = this.writes.get(file);
    const active = before?.active ?? 0;
    const begun = before?.begun ?? 0;
    try {
      const a = await this.json<FileAccessDto>(`${API_FILES}/${file}/access`, isFileAccess);
      if (!fresh()) return;
      const now = this.writes.get(file);
      const own = active > 0 || (now?.active ?? 0) > 0 || (now?.begun ?? 0) !== begun || this.uncertain.has(file);
      this.announce(file, { kind: 'role', role: a.role, version: a.version, canShare: a.canShare === true, own });
    } catch (e) {
      if (fresh()) this.note(file, e);
    }
  }

  /** Новое наблюдение начинается с чистого листа: ответы на прежние запросы ему уже не пишут. */
  forgetAccess(file: string): void {
    this.lastAccess.delete(file);
    this.accessAnswered.set(file, ++this.accessSeq);
  }

  /** Открытый файл: первое состояние приходит всегда, что бы ни запомнили прежние наблюдения и записи. */
  watchAccess(file: string): Promise<void> {
    this.forgetAccess(file);
    return this.checkAccess(file);
  }

  /** Запись, которую откатываем: не ответит — операция всё равно завершается. */
  private async discard(file: string): Promise<void> {
    const ctl = new AbortController();
    const timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
    try {
      await this.http(`${API_FILES}/${file}`, { method: 'DELETE', signal: ctl.signal });
    } catch {
      // Откат не удался: пустой файл останется в списке, его можно удалить вручную.
    } finally {
      this.clock.clearTimeout(timer);
    }
  }

  async list(section: string, query: string, start: number, count: number): Promise<IoResults['list']> {
    if (!parseSection(section)) throw new ServerFailure('BAD_REQUEST');
    const q = new URLSearchParams({ section, q: query, start: String(start), count: String(count) });
    const r = await this.json(`${API_FILES}?${q}`, isFileList);
    return { total: r.total, rows: r.rows.map(toMeta) };
  }

  async get(file: string): Promise<FileMeta | null> {
    try {
      return toMeta(await this.json(`${API_FILES}/${file}`, isFileMeta));
    } catch (e) {
      if (e instanceof ServerFailure && e.code === 'NOT_FOUND') return null;
      throw e;
    }
  }

  /** Новая запись; с содержимым — запись вслед за ней. Не вышло — запись не остаётся пустым файлом. */
  async create(name: string, content: Blob | ArrayBuffer | null, project: string | null, journal?: ArrayBuffer[]): Promise<FileMeta> {
    let meta = toMeta(await this.post(API_FILES, isFileMeta, { name, projectId: project }));
    if (content) {
      if ((content instanceof Blob ? content.size : content.byteLength) > MAX_OPEN_BYTES) {
        await this.discard(meta.id);
        throw new ServerFailure('TOO_LARGE');
      }
      try {
        meta = await this.upload(meta.id, 0, null, content);
      } catch (e) {
        await this.discard(meta.id);
        throw e;
      }
    }
    // Перенос файла из браузера: правки после его байтов становятся журналом нового файла.
    // Не дописались — переноса не было: файл без правок не должен выглядеть перенесённым.
    if (journal) {
      try {
        await Promise.all(journal.map((bytes, i) => this.journal.append(meta.id, 1, i, bytes)));
      } catch (e) {
        await this.journal.drop(meta.id).catch(() => {});
        await this.discard(meta.id);
        throw e;
      }
    }
    this.events.changed();
    return meta;
  }

  /** Содержимое целиком (картинки — на своём месте в контейнере) и поколение именно этих байтов. */
  private async download(file: string): Promise<{ bytes: ArrayBuffer | null; version: number }> {
    const ctl = new AbortController();
    let timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
    try {
      let res: Response;
      try {
        res = await this.http(`${API_FILES}/${file}/content`, { signal: ctl.signal });
      } catch {
        throw new ServerFailure('NETWORK');
      }
      if (res.status !== 200 && res.status !== 204) {
        await this.readReply(res, (_v): _v is never => false);
        throw new ServerFailure('NETWORK');
      }
      const version = Number(res.headers.get(VERSION_HEADER));
      if (res.headers.get(VERSION_HEADER) === null || !Number.isFinite(version)) throw new ServerFailure('NETWORK');
      if (res.status === 204) return { bytes: null, version };
      const declared = Number(res.headers.get('content-length') ?? 0);
      if (declared > MAX_OPEN_BYTES) throw new ServerFailure('TOO_LARGE');
      this.clock.clearTimeout(timer);
      timer = this.clock.setTimeout(() => ctl.abort(), transferTimeout(declared));
      try {
        const body = await res.arrayBuffer();
        return { bytes: await this.withImages(file, body), version };
      } catch (e) {
        if (e instanceof ServerFailure) throw e;
        throw new ServerFailure('NETWORK');
      }
    } finally {
      this.clock.clearTimeout(timer);
    }
  }

  /** Заголовки — как небольшой запрос, тело — по размеру: застывшее соединение не держит заставку. */
  private async image(file: string, hash: string): Promise<ArrayBuffer | null> {
    const ctl = new AbortController();
    let timer: unknown;
    try {
      // Срок — только на сам запрос: пауза после 429 бывает дольше него и обрывом не считается.
      const res = await this.paced(async () => {
        timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
        const reply = await this.http(`${API_FILES}/${file}/images/${hash}`, { signal: ctl.signal });
        this.clock.clearTimeout(timer);
        return reply;
      });
      timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
      if (res.status === 404) return null;
      if (!res.ok) {
        await this.readReply(res, (_v): _v is never => false);
        throw new ServerFailure('NETWORK');
      }
      const declared = Number(res.headers.get('content-length') ?? 0);
      this.clock.clearTimeout(timer);
      timer = this.clock.setTimeout(() => ctl.abort(), transferTimeout(Number.isFinite(declared) ? declared : 0));
      return await res.arrayBuffer();
    } catch (e) {
      if (e instanceof ServerFailure) throw e;
      throw new ServerFailure('NETWORK');
    } finally {
      this.clock.clearTimeout(timer);
    }
  }

  /** Картинки приходят отдельно и встают в контейнер обычными секциями: ядро получает обычный файл.
   * Заголовок секции собирается без вычислений: её хэш — это её имя. Нет картинки — ядро покажет
   * заглушку, как у файла с потерянной секцией. */
  private async withImages(file: string, body: ArrayBuffer): Promise<ArrayBuffer> {
    const source = new Uint8Array(body);
    let spans: SectionSpan[];
    try {
      spans = await scanContainer(source.length, async (s, e) => source.subarray(s, e));
    } catch {
      // Испорченный файл откроет или отвергнет ядро: своё сообщение у него.
      return body;
    }
    const hashes = await imageRefs({ read: async (s, e) => source.subarray(s, e) }, spans);
    if (hashes.size === 0) return body;
    const found: { hash: string; bytes: Uint8Array }[] = [];
    await inPool([...hashes], IMAGE_CONCURRENCY, async (hash) => {
      const got = await this.image(file, hash);
      if (got) found.push({ hash, bytes: new Uint8Array(got) });
    });
    const headers = found.map((f) => imageSectionHeader(f.hash, f.bytes.length));
    const total = source.length + found.reduce((n, f, i) => n + f.bytes.length + headers[i].length, 0);
    if (total > MAX_OPEN_BYTES) throw new ServerFailure('TOO_LARGE');
    const out = new Uint8Array(total);
    out.set(containerHeader(spans.length + found.length));
    out.set(source.subarray(16), 16);
    let at = source.length;
    found.forEach((f, i) => {
      out.set(headers[i], at);
      at += headers[i].length;
      out.set(f.bytes, at);
      at += f.bytes.length;
      this.images(file).add(f.hash);
    });
    return out.buffer;
  }

  private images(file: string): Set<string> {
    let set = this.knownImages.get(file);
    if (!set) this.knownImages.set(file, (set = new Set()));
    return set;
  }

  async read(file: string): Promise<IoResults['read']> {
    const m = await this.json(`${API_FILES}/${file}`, isFileMeta);
    const { bytes, version } = await this.download(file);
    const j: JournalEntries = bytes ? await this.journal.read(file) : { entries: [], session: null, tooLarge: false };
    // Читатель правок не применяет: журнал остаётся до тех пор, пока ему вернут право. Сеанс нужен и ему:
    // новые пачки идут после прежних.
    const held = m.role === 'viewer' && (j.tooLarge || j.entries.length > 0);
    // Читатель журнал не применил: его подтверждение ничего о нём не значит, пачки ждут, пока вернут право.
    this.readKeys.set(file, m.role === 'viewer' ? [] : (j.keys ?? []));
    if (held) this.heldJournal.add(file);
    else this.heldJournal.delete(file);
    const applied = m.role !== 'viewer';
    return {
      // Поколение — именно этих байтов: запись на их основе проверяется по нему.
      meta: toMeta({ ...m, version }),
      bytes,
      journal: applied ? j.entries : [],
      session: j.session,
      journalTooLarge: applied && j.tooLarge,
      journalHeld: held,
    };
  }

  /** Файл `.ferrum` с картинками: «Скачать копию» (читателю тоже, раздел 6.1). */
  async blob(file: string): Promise<Blob | null> {
    // Правки могут быть ещё в журнале сервера: сначала он собирает файл целиком. Срок — как у передачи всего файла.
    let compacted: Response;
    try {
      compacted = await this.http(`${API_FILES}/${file}/compact`, {
        method: 'POST',
        signal: AbortSignal.timeout(transferTimeout(MAX_OPEN_BYTES)),
      });
    } catch {
      throw new ServerFailure('NETWORK');
    }
    // Сервер сжимает файл или недавно сжал: просит подождать (`Retry-After`), а не недоступен.
    const retry = Number(compacted.headers.get('retry-after'));
    if (compacted.status === 503 && Number.isFinite(retry) && retry > 0) {
      void compacted.body?.cancel().catch(() => {});
      throw new ServerFailure('UNAVAILABLE', `Копия собирается — повторите через ${Math.ceil(retry)} с`);
    }
    // Журнал документа, который видят люди, в файл не вошёл: копия была бы неполной, повтор не поможет.
    if (compacted.status === 503) {
      const body: unknown = await compacted.json().catch(() => null);
      throw new ServerFailure(record(body)?.error === 'INCOMPLETE' ? 'INCOMPLETE' : 'UNAVAILABLE');
    }
    if (!compacted.ok) await this.readReply(compacted, (_v): _v is never => false);
    const { bytes } = await this.download(file);
    return bytes ? new Blob([bytes], { type: 'application/x-ferrum' }) : null;
  }

  private async uploadImages(file: string, images: Split['images']): Promise<void> {
    const known = this.images(file);
    await inPool(images, IMAGE_CONCURRENCY, async (img) => {
      if (known.has(img.hash)) return;
      const url = `${API_FILES}/${file}/images/${img.hash}`;
      let res: Response;
      try {
        res = await this.paced(() => this.http(url, { method: 'HEAD', signal: AbortSignal.timeout(JSON_TIMEOUT_MS) }));
        if (res.status === 404) {
          res = await this.paced(() =>
            this.http(url, { method: 'PUT', body: new Blob([img.part]), signal: AbortSignal.timeout(transferTimeout(img.size)) }),
          );
        }
      } catch {
        throw new ServerFailure('NETWORK');
      }
      if (!res.ok) {
        // Ответ HEAD без тела: причину скажет проверка доступа, текст — сам код.
        if (res.status === 404 || res.status === 401 || res.status === 403 || res.status === 429) {
          const codes = { 404: 'NOT_FOUND', 401: 'UNAUTHORIZED', 403: 'FORBIDDEN', 429: 'TOO_MANY_REQUESTS' } as const;
          throw new ServerFailure(codes[res.status]);
        }
        await this.readReply(res, (_v): _v is never => false);
      }
      known.add(img.hash);
    });
  }

  /** Заведомый отказ — раньше передачи: файл могут записать другие, и незачем гнать десятки мегабайт.
   * Поколение показывает, сколько записей с неизвестным исходом дошло: `k`-я сделала его, более поздние
   * не дошли. Запись идёт поверх неё; если поколение сделал кто-то другой, откажет сервер. */
  private async preflight(file: string, base: number): Promise<{ base: number; previous: string | null }> {
    const a = await this.json<FileAccessDto>(`${API_FILES}/${file}/access`, isFileAccess);
    if (a.role === 'viewer') throw new ServerFailure('FORBIDDEN');
    const chain = this.uncertain.get(file) ?? [];
    const landed = a.version - base;
    if (landed === 0) {
      // Ни одна не дошла к этому моменту; последняя ещё может дойти, поэтому её номер едет с записью.
      this.uncertain.delete(file);
      return { base, previous: chain.at(-1) ?? null };
    }
    if (landed < 0 || landed > chain.length) throw new ServerFailure('CONFLICT');
    this.uncertain.set(file, chain.slice(0, landed));
    return { base: a.version - 1, previous: chain[landed - 1] };
  }

  private async upload(file: string, base: number, previous: string | null, content: Blob | ArrayBuffer): Promise<FileMeta> {
    const source = content instanceof Blob ? blobSource(content) : bytesSource(new Uint8Array(content));
    const split = await splitContainer(source, () => content as BodyInit);
    // Заведомый отказ сервера: ни картинки, ни тело не передаются зря.
    if (split.refs > MAX_IMAGE_REFS) throw new ServerFailure('TOO_MANY_IMAGES');
    await this.uploadImages(file, split.images);
    try {
      return await this.putContent(file, base, previous, split);
    } catch (e) {
      if (!(e instanceof ServerFailure) || e.code !== 'MISSING_IMAGES') throw e;
    }
    // Запись не прошла: кэш врёт (очистка сняла картинку, пока вкладка была открыта). Сверяем всё заново.
    this.images(file).clear();
    await this.uploadImages(file, split.images);
    return this.putContent(file, base, previous, split);
  }

  private async putContent(file: string, base: number, previous: string | null, split: Split): Promise<FileMeta> {
    const write = this.newId();
    this.uncertain.set(file, [...(this.uncertain.get(file) ?? []), write]);
    let res: Response;
    try {
      res = await this.http(`${API_FILES}/${file}/content`, {
        method: 'PUT',
        headers: {
          'If-Match': `"${base}"`,
          [WRITE_ID_HEADER]: write,
          ...(previous ? { [PREVIOUS_WRITE_HEADER]: previous } : {}),
          ...(split.lost > 0 ? { [LOST_IMAGES_HEADER]: String(split.lost) } : {}),
        },
        body: split.body,
        signal: AbortSignal.timeout(transferTimeout(split.size)),
      });
    } catch {
      // Ответа нет: запись могла пройти. Номер остаётся: следующая пройдёт и в этом случае.
      throw new ServerFailure('NETWORK');
    }
    // Ответ есть: исход известен.
    if (res.status < 500) this.uncertain.delete(file);
    return toMeta(await this.readReply(res, isFileMeta));
  }

  async write(
    file: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ): Promise<FileMeta & { warning?: string }> {
    const state = this.writes.get(file) ?? { active: 0, begun: 0 };
    this.writes.set(file, state);
    state.active++;
    state.begun++;
    try {
      // Сервер считает файл без картинок, а открыть его целиком потом будет нельзя: прежнее содержимое остаётся,
      // правки — в журнале браузера.
      if (bytes.byteLength > MAX_OPEN_BYTES) throw new ServerFailure('TOO_LARGE');
      const from = await this.preflight(file, base);
      const meta = await this.upload(file, from.base, from.previous, bytes);
      let warning: string | undefined;
      if (journal && !this.heldJournal.has(file)) {
        // Файл уже записан: сбой здесь не отказ записи. Оставшиеся пачки снимет следующая, а открытие
        // применит их поверх файла без вреда.
        await this.journal.cut(file, journal.session, journal.upTo).catch(() => {
          warning = 'Файл записан, но журнал изменений не очищен';
        });
      }
      if (thumb !== 'keep') this.offerThumb(file, thumb);
      this.events.changed();
      return warning ? { ...meta, warning } : meta;
    } catch (e) {
      this.note(file, e);
      throw e;
    } finally {
      state.active--;
    }
  }

  /** Превью не чаще раза в минуту: новое до срока заменяет ждущее. Сбой не портит запись файла. */
  private offerThumb(file: string, thumb: ArrayBuffer | null): void {
    if (thumb && thumb.byteLength > MAX_THUMB_BYTES) return;
    const state = this.thumbs.get(file) ?? { last: -Infinity, pending: undefined, timer: null };
    this.thumbs.set(file, state);
    state.pending = thumb;
    if (state.timer !== null) return;
    const wait = state.last + THUMB_INTERVAL_MS - this.clock.now();
    if (wait <= 0) {
      void this.sendThumb(file);
      return;
    }
    state.timer = this.clock.setTimeout(() => {
      state.timer = null;
      void this.sendThumb(file);
    }, wait);
  }

  private async sendThumb(file: string): Promise<void> {
    const state = this.thumbs.get(file);
    if (!state || state.pending === undefined) return;
    const thumb = state.pending;
    state.pending = undefined;
    // Убрать превью (страница пуста) — не повод откладывать первое настоящее.
    if (thumb) state.last = this.clock.now();
    try {
      const res = await this.http(
        `${API_FILES}/${file}/thumb`,
        thumb
          ? { method: 'PUT', body: thumb, signal: AbortSignal.timeout(JSON_TIMEOUT_MS) }
          : { method: 'DELETE', signal: AbortSignal.timeout(JSON_TIMEOUT_MS) },
      );
      if (res.ok) this.events.changed();
    } catch {
      // Превью обновится со следующей записью.
    }
  }

  /** Картинка новой правки (`sync.worker`): раньше самой правки. Есть у файла — байты не идут. */
  async uploadOneImage(file: string, hash: string, bytes: Uint8Array): Promise<void> {
    await this.uploadImages(file, [{ hash, size: bytes.length, part: bytes as Uint8Array<ArrayBuffer> }]);
  }

  /** Картинка по чужой ссылке. Нет у файла — `null`. */
  downloadOneImage(file: string, hash: string): Promise<ArrayBuffer | null> {
    return this.image(file, hash);
  }

  /** Снимок одного документа: клиент отстал сильнее журнала. Сверх предела частоты ждёт, сколько просит сервер. Сроки — как у
   * картинки: заголовки — как небольшой запрос, тело — по размеру; застывший ответ не держит отправку правок. */
  async fetchSnapshot(path: string): Promise<ArrayBuffer> {
    const ctl = new AbortController();
    let timer: unknown;
    try {
      let res: Response;
      try {
        // Срок — только на сам запрос: пауза после 429 бывает дольше него и обрывом не считается.
        res = await this.paced(async () => {
          timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
          const reply = await this.http(path, { signal: ctl.signal });
          this.clock.clearTimeout(timer);
          return reply;
        });
      } catch {
        throw new ServerFailure('NETWORK');
      }
      timer = this.clock.setTimeout(() => ctl.abort(), JSON_TIMEOUT_MS);
      if (!res.ok) {
        await this.readReply(res, (_v): _v is never => false);
        throw new ServerFailure('NETWORK');
      }
      // Снимок идёт потоком без `Content-Length`: длину сервер называет своим заголовком.
      const declared = Number(res.headers.get('content-length') ?? res.headers.get('x-ferrum-size') ?? 0);
      if (declared > MAX_OPEN_BYTES) throw new ServerFailure('TOO_LARGE');
      this.clock.clearTimeout(timer);
      timer = this.clock.setTimeout(() => ctl.abort(), transferTimeout(Number.isFinite(declared) ? declared : 0));
      try {
        return await res.arrayBuffer();
      } catch {
        throw new ServerFailure('NETWORK');
      }
    } finally {
      this.clock.clearTimeout(timer);
    }
  }

  /** Доступ сейчас, без памяти наблюдений. Сеть — `null`: это не сообщение о доступе. */
  async accessOnce(file: string): Promise<AccessState | null> {
    try {
      const a = await this.json<FileAccessDto>(`${API_FILES}/${file}/access`, isFileAccess);
      return { kind: 'role', role: a.role, version: a.version, canShare: a.canShare === true, own: true };
    } catch (e) {
      if (e instanceof ServerFailure && e.code === 'NOT_FOUND') return { kind: 'none' };
      if (e instanceof ServerFailure && e.code === 'UNAUTHORIZED') return { kind: 'session' };
      return null;
    }
  }

  /** Сервер подтвердил правки журнала до пачки `upTo` этого сеанса (0 — только прочитанное при открытии). Прочитанное при открытии
   * снимается при любом `upTo`: ядро подтверждает отметки по порядку и сообщает последнюю, так что отметка N значит и отметку 0.
   * Чужие сеансы не трогает: это другая вкладка, а её правки могут быть ещё только здесь. */
  async cutJournal(file: string, session: number, upTo: number): Promise<void> {
    const keys = this.readKeys.get(file) ?? [];
    this.readKeys.delete(file);
    const own: JournalKey[] = Array.from({ length: upTo }, (_, i): JournalKey => [file, session, i + 1]);
    if (keys.length + own.length > 0) await this.journal.cutExact([...keys, ...own]);
  }

  /** Превью при правках журналом: файл целиком больше не пишется. */
  putThumb(file: string, thumb: ArrayBuffer | null): void {
    this.offerThumb(file, thumb);
  }

  appendJournal(file: string, session: number, batch: number, bytes: ArrayBuffer): Promise<void> {
    return this.journal.append(file, session, batch, bytes);
  }

  async rename(file: string, name: string): Promise<FileMeta> {
    const m = toMeta(
      await this.json(`${API_FILES}/${file}`, isFileMeta, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      }),
    );
    this.events.changed();
    return m;
  }

  async touch(file: string): Promise<FileMeta> {
    return toMeta(await this.post(`${API_FILES}/${file}/touch`, isFileMeta));
  }

  private async act(path: string, method = 'POST'): Promise<void> {
    await this.json(path, (v): v is unknown => record(v) !== null, { method });
    this.events.changed();
  }

  trash(file: string): Promise<void> {
    return this.act(`${API_FILES}/${file}/trash`);
  }
  restore(file: string): Promise<void> {
    return this.act(`${API_FILES}/${file}/restore`);
  }
  async remove(file: string): Promise<void> {
    await this.act(`${API_FILES}/${file}`, 'DELETE');
    await this.journal.drop(file);
  }
  emptyTrash(): Promise<void> {
    return this.act(`${API_FILES}/empty-trash`);
  }

  async thumbnail(file: string): Promise<Blob | null> {
    let res: Response;
    try {
      res = await this.http(`${API_FILES}/${file}/thumb`, { signal: AbortSignal.timeout(JSON_TIMEOUT_MS) });
    } catch {
      throw new ServerFailure('NETWORK');
    }
    if (res.status === 204 || res.status === 404) return null;
    if (!res.ok) throw new ServerFailure(res.status >= 500 ? 'UNAVAILABLE' : res.status === 429 ? 'TOO_MANY_REQUESTS' : 'NETWORK');
    return res.blob();
  }
}
