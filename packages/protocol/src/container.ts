// Контейнер `.ferrum` без разбора данных (формат — `crates/ferrum-doc/src/container.rs`):
// заголовок и секции `тип u8 · длина id u32 · id · длина данных u64 · blake3 32 байта · данные`.
// Сервер идёт по нему потоком, не держа файл в памяти; клиент выносит картинки в отдельные запросы
// и собирает файл обратно: хэш картинки — её id, поэтому заголовок секции собирается без вычислений.

export const CONTAINER_MAGIC = new Uint8Array([70, 69, 82, 82, 85, 77, 0, 0]);
export const CONTAINER_VERSION = 1;
export const SECTION_PAGE_REFS = 3;
export const SECTION_IMAGE = 4;
/** Заголовок файла: магия, версия, число секций. */
export const CONTAINER_HEADER_BYTES = 16;
/** Секция до id: тип и длина id. */
export const SECTION_PREFIX_BYTES = 5;
/** Секция после id: длина данных и blake3. */
export const SECTION_SUFFIX_BYTES = 40;
const MAX_SECTION_ID_BYTES = 1024;
const MAX_REFS_BYTES = 1024 * 1024;
const MAX_SECTIONS = 1_000_000;
/** Разных картинок в файле. Настоящий большой файл — тысячи; предел держит память записи постоянной: список
 * ссылок копится весь проход, а один файл до 512 МБ вместил бы миллионы. См. также `LIMITS.images` сервера:
 * файл на пределе открывается и записывается меньше чем за две минуты. */
export const MAX_IMAGE_REFS = 10_000;
const HASH_LINE_RE = /^[0-9a-f]{64}$/;

export class ContainerError extends Error {}
/** Контейнер верный, но картинок в нём больше `MAX_IMAGE_REFS`. */
export class TooManyImages extends ContainerError {}

const utf8 = new TextDecoder();

/** Проход по контейнеру кусками, какими они пришли из сети: секция картинки — ошибка (картинки идут
 * отдельно, спека, раздел 6.4), неполный файл и лишние байты — тоже. Память — только заголовки и
 * списки ссылок страниц. */
export class ContainerWalker {
  private head = new Uint8Array(CONTAINER_HEADER_BYTES);
  private filled = 0;
  private state: 'header' | 'prefix' | 'id' | 'suffix' | 'data' | 'refs' | 'end' = 'header';
  private need = CONTAINER_HEADER_BYTES;
  private left = 0n;
  private sections = 0;
  private kind = 0;
  private refs = new Uint8Array(0);
  private refsFilled = 0;
  /** Хэши картинок, на которые ссылаются страницы. */
  readonly imageRefs = new Set<string>();

  private ensure(size: number): void {
    if (this.head.length < size) this.head = new Uint8Array(size);
  }

  feed(chunk: Uint8Array): void {
    let at = 0;
    while (at < chunk.length) {
      if (this.state === 'end') throw new ContainerError('лишние байты в конце');
      if (this.state === 'data') {
        const take = Number(this.left < BigInt(chunk.length - at) ? this.left : BigInt(chunk.length - at));
        this.left -= BigInt(take);
        at += take;
        if (this.left === 0n) this.next();
        continue;
      }
      if (this.state === 'refs') {
        const take = Math.min(this.refs.length - this.refsFilled, chunk.length - at);
        this.refs.set(chunk.subarray(at, at + take), this.refsFilled);
        this.refsFilled += take;
        at += take;
        if (this.refsFilled === this.refs.length) {
          this.takeRefs();
          this.next();
        }
        continue;
      }
      const take = Math.min(this.need - this.filled, chunk.length - at);
      this.head.set(chunk.subarray(at, at + take), this.filled);
      this.filled += take;
      at += take;
      if (this.filled === this.need) this.step();
    }
  }

  private step(): void {
    const view = new DataView(this.head.buffer, this.head.byteOffset, this.filled);
    this.filled = 0;
    switch (this.state) {
      case 'header': {
        for (let i = 0; i < CONTAINER_MAGIC.length; i++) {
          if (this.head[i] !== CONTAINER_MAGIC[i]) throw new ContainerError('это не файл Ferrum');
        }
        if (view.getUint32(8, true) !== CONTAINER_VERSION) throw new ContainerError('неизвестная версия формата');
        this.sections = view.getUint32(12, true);
        if (this.sections > MAX_SECTIONS) throw new ContainerError('слишком много секций');
        this.next();
        return;
      }
      case 'prefix': {
        this.kind = view.getUint8(0);
        if (this.kind === SECTION_IMAGE) throw new ContainerError('картинка внутри файла');
        const idBytes = view.getUint32(1, true);
        if (idBytes > MAX_SECTION_ID_BYTES) throw new ContainerError('слишком длинный id секции');
        this.state = idBytes > 0 ? 'id' : 'suffix';
        this.need = idBytes > 0 ? idBytes : SECTION_SUFFIX_BYTES;
        this.ensure(this.need);
        return;
      }
      case 'id':
        this.state = 'suffix';
        this.need = SECTION_SUFFIX_BYTES;
        this.ensure(this.need);
        return;
      case 'suffix': {
        const size = view.getBigUint64(0, true);
        if (this.kind === SECTION_PAGE_REFS) {
          if (size > BigInt(MAX_REFS_BYTES)) throw new ContainerError('слишком длинный список ссылок');
          if (size === 0n) {
            this.next();
            return;
          }
          this.refs = new Uint8Array(Number(size));
          this.refsFilled = 0;
          this.state = 'refs';
          return;
        }
        if (size === 0n) {
          this.next();
          return;
        }
        this.left = size;
        this.state = 'data';
        return;
      }
      default:
        throw new ContainerError('неверное состояние');
    }
  }

  private takeRefs(): void {
    for (const line of utf8.decode(this.refs).split('\n')) {
      if (!HASH_LINE_RE.test(line)) continue;
      this.imageRefs.add(line);
      if (this.imageRefs.size > MAX_IMAGE_REFS) throw new TooManyImages('слишком много картинок');
    }
    this.refs = new Uint8Array(0);
  }

  private next(): void {
    if (this.sections === 0) {
      this.state = 'end';
      return;
    }
    this.sections--;
    this.state = 'prefix';
    this.need = SECTION_PREFIX_BYTES;
    this.ensure(this.need);
  }

  /** Весь файл пришёл: секций столько, сколько обещано, ни одна не оборвана. */
  finish(): void {
    if (this.state !== 'end') throw new ContainerError('файл обрезан');
  }
}

/** Заголовок секции картинки: тип, id (хэш), длина, blake3 (тот же хэш). */
export function imageSectionHeader(hash: string, size: number): Uint8Array {
  const id = new TextEncoder().encode(hash);
  const out = new Uint8Array(SECTION_PREFIX_BYTES + id.length + SECTION_SUFFIX_BYTES);
  const view = new DataView(out.buffer);
  out[0] = SECTION_IMAGE;
  view.setUint32(1, id.length, true);
  out.set(id, SECTION_PREFIX_BYTES);
  view.setBigUint64(SECTION_PREFIX_BYTES + id.length, BigInt(size), true);
  for (let i = 0; i < 32; i++) {
    out[SECTION_PREFIX_BYTES + id.length + 8 + i] = parseInt(hash.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** Заголовок файла с другим числом секций: остальное копируется как есть. */
export function containerHeader(sections: number): Uint8Array {
  const out = new Uint8Array(CONTAINER_HEADER_BYTES);
  const view = new DataView(out.buffer);
  out.set(CONTAINER_MAGIC);
  view.setUint32(8, CONTAINER_VERSION, true);
  view.setUint32(12, sections, true);
  return out;
}

/** Секция файла: где заголовок, где данные. */
export interface SectionSpan {
  kind: number;
  id: string;
  /** Начало заголовка секции. */
  start: number;
  /** Начало данных. */
  data: number;
  size: number;
}

/** Разбор заголовков по кускам файла, которые читает `read`: файл целиком в память не загружается
 * (на клиенте `read` — это `Blob.slice`). Неверный контейнер — `ContainerError`. */
export async function scanContainer(total: number, read: (start: number, end: number) => Promise<Uint8Array>): Promise<SectionSpan[]> {
  if (total < CONTAINER_HEADER_BYTES) throw new ContainerError('это не файл Ferrum');
  const head = await read(0, CONTAINER_HEADER_BYTES);
  for (let i = 0; i < CONTAINER_MAGIC.length; i++) {
    if (head[i] !== CONTAINER_MAGIC[i]) throw new ContainerError('это не файл Ferrum');
  }
  const view = new DataView(head.buffer, head.byteOffset, head.byteLength);
  if (view.getUint32(8, true) !== CONTAINER_VERSION) throw new ContainerError('неизвестная версия формата');
  const count = view.getUint32(12, true);
  if (count > MAX_SECTIONS) throw new ContainerError('слишком много секций');
  const spans: SectionSpan[] = [];
  let at = CONTAINER_HEADER_BYTES;
  for (let i = 0; i < count; i++) {
    if (at + SECTION_PREFIX_BYTES > total) throw new ContainerError('файл обрезан');
    const prefix = await read(at, at + SECTION_PREFIX_BYTES);
    const pv = new DataView(prefix.buffer, prefix.byteOffset, prefix.byteLength);
    const idBytes = pv.getUint32(1, true);
    if (idBytes > MAX_SECTION_ID_BYTES) throw new ContainerError('слишком длинный id секции');
    const dataAt = at + SECTION_PREFIX_BYTES + idBytes + SECTION_SUFFIX_BYTES;
    if (dataAt > total) throw new ContainerError('файл обрезан');
    const rest = await read(at + SECTION_PREFIX_BYTES, dataAt);
    const size = Number(new DataView(rest.buffer, rest.byteOffset, rest.byteLength).getBigUint64(idBytes, true));
    if (dataAt + size > total) throw new ContainerError('файл обрезан');
    spans.push({ kind: prefix[0], id: utf8.decode(rest.subarray(0, idBytes)), start: at, data: dataAt, size });
    at = dataAt + size;
  }
  if (at !== total) throw new ContainerError('лишние байты в конце');
  return spans;
}
