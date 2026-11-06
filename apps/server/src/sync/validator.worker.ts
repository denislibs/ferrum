// Проверка пачек перед журналом (спека подсистемы 4, раздел 7.3): байты разбираются и применяются к настоящему
// документу файла. Пачка, которая разбирается на пустом документе, может уронить Loro на настоящем (проба: 1 из
// 3619 случайных порч), и журнал отравил бы файл всем. Паника wasm портит весь модуль, поэтому worker отдельный:
// сервер его заменяет и пачку отвергает.
//
// Документ здесь — ровно журнал сервера плюс пачки, которые журнал ещё не решил. Пачка, которая ждёт неизвестного
// (`pending`), или которую отверг журнал, остаётся в документе Loro навсегда, поэтому документ уходит: следующая пачка
// загрузит его из журнала заново.

import { decodeImportBlobMeta, LoroDoc } from 'loro-crdt';
// @ts-expect-error у сборки wasm нет типов: нужен только размер памяти модуля.
import { __wasm } from 'loro-crdt/nodejs/loro_wasm.js';

/** Без пачек дольше этого документ уходит из памяти, следующая пачка загрузит его заново. */
const IDLE_MS = 120_000;
/** Дольше этого идёт только параллельная правка дерева: Loro откатывает всё дерево. */
const HEAVY_IMPORT_MS = 30;
/** Документ с кэшем истории уходит раньше: в `loro-crdt` кэш отдельно не освободить. */
const HEAVY_IDLE_MS = 5000;

export type ToValidator =
  /** Настройка worker'а: первое сообщение после запуска, ответа нет. */
  | { id: 0; op: 'config'; budget: number }
  | { id: number; op: 'validate'; key: string; bytes: Uint8Array }
  /** Начало загрузки: снимок; журнал идёт следом кусками `append`. `epoch` — номер загрузки, у сервера он свой на каждую. */
  | { id: number; op: 'load'; key: string; epoch: number; snapshot: Uint8Array | null }
  /** Кусок журнала подряд: `log` — байты записей одна за другой, `sizes` — их длины. */
  | { id: number; op: 'append'; key: string; epoch: number; log: Uint8Array; sizes: number[] }
  /** Журнал отверг пачку этой загрузки: документ уходит. */
  | { id: number; op: 'drop'; key: string; epoch: number }
  | { id: number; op: 'forget'; prefix: string }
  | { id: number; op: 'base'; snapshot: Uint8Array };

export type FromValidator = (
  | { id: number; ok: true; start: Record<string, number>; end: Record<string, number>; pending: boolean; epoch: number }
  /** Документа нет в памяти: сервер пришлёт его и повторит. */
  | { id: number; ok: false; reason: 'need-load' }
  /** Плохие байты. `fatal` — Loro упал: модуль испорчен, worker надо заменить. */
  | { id: number; ok: false; reason: 'bad'; fatal: boolean }
  /** Паника на снимке или журнале самого файла: worker заменить, пачку не винить. */
  | { id: number; ok: false; reason: 'fatal'; message: string }
  /** `base: null` — снимок не читается (чужие байты). */
  | { id: number; ok: true; base: Record<string, number> | null }
  | { id: number; ok: true; done: true }
) & {
  /** Память модуля wasm: она не возвращается системе, сервер по ней меняет worker. */
  memory?: number;
};

interface Held {
  /** `null` — снимок не читается (его записал не Ferrum): проверяем только разбор, файл живёт как жил. */
  doc: LoroDoc | null;
  epoch: number;
  bytes: number;
  used: number;
  /** Loro построил кэш истории (параллельная правка дерева): на большом документе это сотни мегабайт. */
  heavy: boolean;
}

const scope = globalThis as unknown as Worker;
const docs = new Map<string, Held>();
let held = 0;
/** Документов в памяти на столько байт снимков и журнала: дальше вытесняются давние. */
let budget = 256 * 1024 * 1024;

const plain = (m: { toJSON(): Map<string, number> }): Record<string, number> => Object.fromEntries(m.toJSON());
const memory = (): number => (__wasm.memory as WebAssembly.Memory).buffer.byteLength;
const isPanic = (e: unknown): boolean => e instanceof WebAssembly.RuntimeError || (e instanceof Error && /unreachable/i.test(e.message));

function drop(key: string): void {
  const h = docs.get(key);
  if (!h) return;
  docs.delete(key);
  held -= h.bytes;
  h.doc?.free();
}

function makeRoom(need: number, keep?: string): void {
  const oldest = [...docs.entries()].sort(([, a], [, b]) => a.used - b.used);
  for (const [key] of oldest) {
    if (held + need <= budget) return;
    if (key !== keep) drop(key);
  }
}

function load(key: string, epoch: number, snapshot: Uint8Array | null): void {
  drop(key);
  const bytes = snapshot?.length ?? 0;
  makeRoom(bytes);
  let doc: LoroDoc | null = null;
  try {
    doc = snapshot && snapshot.length > 0 ? LoroDoc.fromSnapshot(snapshot) : new LoroDoc();
  } catch (e) {
    if (isPanic(e)) throw e;
    // Снимок записал не Ferrum: правки идут как шли, проверяется только разбор.
    doc = null;
  }
  docs.set(key, { doc, epoch, bytes, used: performance.now(), heavy: false });
  held += bytes;
}

function append(key: string, epoch: number, log: Uint8Array, sizes: number[]): void {
  const h = docs.get(key);
  if (!h || h.epoch !== epoch) return;
  makeRoom(log.length, key);
  let at = 0;
  for (const size of sizes) {
    const b = log.subarray(at, at + size);
    at += size;
    if (!h.doc) continue;
    try {
      h.doc.import(b);
    } catch (e) {
      if (isPanic(e)) throw e;
      // Запись журнала не применилась: документ остаётся (проверка не слабеет), правки за ней будут ждать.
      console.error('Проверка пачек: запись журнала не применилась при загрузке', key, e instanceof Error ? e.message : e);
    }
  }
  h.bytes += log.length;
  held += log.length;
}

function validate(id: number, key: string, bytes: Uint8Array): FromValidator {
  const h = docs.get(key);
  if (!h) return { id, ok: false, reason: 'need-load' };
  const meta = decodeImportBlobMeta(bytes, false);
  if (meta.mode !== 'update') throw new BadUpdate();
  const start = plain(meta.partialStartVersionVector);
  const end = plain(meta.partialEndVersionVector);
  let pending = false;
  if (h.doc) {
    const at = performance.now();
    let status;
    try {
      status = h.doc.import(bytes);
    } catch (e) {
      // Применилась ли часть, неизвестно: документ уходит. После паники модуль трогать нельзя — уходит весь worker.
      if (!isPanic(e)) drop(key);
      throw e;
    }
    pending = status.pending != null;
    if (performance.now() - at > HEAVY_IMPORT_MS) h.heavy = true;
  } else {
    const scratch = new LoroDoc();
    // После паники модуль трогать нельзя, `free` тоже: worker уходит целиком.
    scratch.import(bytes);
    scratch.free();
  }
  if (pending) {
    // Ждущая пачка осталась бы в документе и потом применилась: документ опередил бы журнал.
    drop(key);
    return { id, ok: true, start, end, pending, epoch: h.epoch };
  }
  h.used = performance.now();
  h.bytes += bytes.length;
  held += bytes.length;
  makeRoom(0, key);
  return { id, ok: true, start, end, pending, epoch: h.epoch };
}

class BadUpdate extends Error {}

/** Конец версий снимка: документ не загружается. Чужие байты — `null`. */
function base(snapshot: Uint8Array): Record<string, number> | null {
  if (snapshot.length === 0) return {};
  try {
    return plain(decodeImportBlobMeta(snapshot, true).partialEndVersionVector);
  } catch (e) {
    if (isPanic(e)) throw e;
    return null;
  }
}

function post(m: FromValidator): void {
  scope.postMessage({ ...m, memory: memory() });
}

scope.onmessage = (e: MessageEvent<ToValidator>) => {
  const m = e.data;
  try {
    switch (m.op) {
      case 'config':
        budget = m.budget;
        return;
      case 'validate':
        post(validate(m.id, m.key, m.bytes));
        return;
      case 'load':
        load(m.key, m.epoch, m.snapshot);
        post({ id: m.id, ok: true, done: true });
        return;
      case 'append':
        append(m.key, m.epoch, m.log, m.sizes);
        post({ id: m.id, ok: true, done: true });
        return;
      case 'drop':
        if (docs.get(m.key)?.epoch === m.epoch) drop(m.key);
        post({ id: m.id, ok: true, done: true });
        return;
      case 'forget':
        for (const key of docs.keys()) if (key.startsWith(m.prefix)) drop(key);
        post({ id: m.id, ok: true, done: true });
        return;
      case 'base':
        post({ id: m.id, ok: true, base: base(m.snapshot) });
        return;
    }
  } catch (err) {
    // Обычная ошибка разбора — плохие байты. Паника wasm (`unreachable`, нехватка памяти) ещё и портит модуль.
    const fatal = isPanic(err);
    if (m.op === 'validate') post({ id: m.id, ok: false, reason: 'bad', fatal });
    else post({ id: m.id, ok: false, reason: 'fatal', message: err instanceof Error ? err.message : String(err) });
  }
};

setInterval(() => {
  const now = performance.now();
  for (const [key, h] of docs) {
    if (now - h.used > (h.heavy ? HEAVY_IDLE_MS : IDLE_MS)) drop(key);
  }
}, 1000);
