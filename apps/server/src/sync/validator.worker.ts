// Проверка пачек перед журналом (спека подсистемы 4, раздел 7.3): байты разбираются и применяются к настоящему
// документу файла. Пачка, которая разбирается на пустом документе, может уронить Loro на настоящем (проба: 1 из
// 3619 случайных порч), и журнал отравил бы файл всем. Паника wasm портит весь модуль, поэтому worker отдельный:
// сервер его заменяет и пачку отвергает.

import { decodeImportBlobMeta, LoroDoc } from 'loro-crdt';

/** Документов в памяти на столько байт снимков и журнала: дальше вытесняются давние. */
const BUDGET_BYTES = Number(process.env.FERRUM_VALIDATOR_BUDGET ?? 512 * 1024 * 1024);
/** Без пачек дольше этого документ уходит из памяти, следующая пачка загрузит его заново. */
const IDLE_MS = 120_000;
/** Дольше этого идёт только параллельная правка дерева: Loro откатывает всё дерево. */
const HEAVY_IMPORT_MS = 30;
/** Документ с кэшем истории уходит раньше: в `loro-crdt` кэш отдельно не освободить. */
const HEAVY_IDLE_MS = 5000;

export type ToValidator =
  | { id: number; op: 'validate'; key: string; bytes: Uint8Array }
  | { id: number; op: 'load'; key: string; snapshot: Uint8Array | null; log: Uint8Array[] }
  | { id: number; op: 'forget'; prefix: string }
  | { id: number; op: 'base'; snapshot: Uint8Array };

export type FromValidator =
  | { id: number; ok: true; start: Record<string, number>; end: Record<string, number>; pending: boolean; loaded: boolean }
  /** Документа нет в памяти: сервер пришлёт его и повторит. */
  | { id: number; ok: false; reason: 'need-load' }
  /** Плохие байты. `fatal` — Loro упал: модуль испорчен, worker надо заменить. */
  | { id: number; ok: false; reason: 'bad'; fatal: boolean }
  | { id: number; ok: true; base: Record<string, number> }
  | { id: number; ok: true; done: true };

interface Held {
  /** `null` — снимок не читается (его записал не Ferrum): проверяем только разбор, файл живёт как жил. */
  doc: LoroDoc | null;
  bytes: number;
  used: number;
  /** Loro построил кэш истории (параллельная правка дерева): на большом документе это сотни мегабайт. */
  heavy: boolean;
}

const scope = globalThis as unknown as Worker;
const docs = new Map<string, Held>();
let held = 0;

const plain = (m: { toJSON(): Map<string, number> }): Record<string, number> => Object.fromEntries(m.toJSON());

function drop(key: string): void {
  const h = docs.get(key);
  if (!h) return;
  docs.delete(key);
  held -= h.bytes;
  h.doc?.free();
}

function makeRoom(need: number): void {
  const oldest = [...docs.entries()].sort(([, a], [, b]) => a.used - b.used);
  for (const [key] of oldest) {
    if (held + need <= BUDGET_BYTES) return;
    drop(key);
  }
}

function load(key: string, snapshot: Uint8Array | null, log: Uint8Array[]): void {
  drop(key);
  const bytes = (snapshot?.length ?? 0) + log.reduce((n, b) => n + b.length, 0);
  makeRoom(bytes);
  let doc: LoroDoc | null;
  try {
    doc = snapshot && snapshot.length > 0 ? LoroDoc.fromSnapshot(snapshot) : new LoroDoc();
    for (const b of log) doc.import(b);
  } catch {
    doc = null;
  }
  docs.set(key, { doc, bytes, used: performance.now(), heavy: false });
  held += bytes;
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
    const status = h.doc.import(bytes);
    pending = status.pending != null;
    if (performance.now() - at > HEAVY_IMPORT_MS) h.heavy = true;
  } else {
    const scratch = new LoroDoc();
    scratch.import(bytes);
    scratch.free();
  }
  h.used = performance.now();
  h.bytes += bytes.length;
  held += bytes.length;
  return { id, ok: true, start, end, pending, loaded: h.doc !== null };
}

class BadUpdate extends Error {}

/** Конец версий снимка: документ не загружается. Чужие байты — пустой вектор. */
function base(snapshot: Uint8Array): Record<string, number> {
  if (snapshot.length === 0) return {};
  try {
    return plain(decodeImportBlobMeta(snapshot, true).partialEndVersionVector);
  } catch {
    return {};
  }
}

scope.onmessage = (e: MessageEvent<ToValidator>) => {
  const m = e.data;
  try {
    switch (m.op) {
      case 'validate':
        scope.postMessage(validate(m.id, m.key, m.bytes));
        return;
      case 'load':
        load(m.key, m.snapshot, m.log);
        scope.postMessage({ id: m.id, ok: true, done: true });
        return;
      case 'forget':
        for (const key of docs.keys()) if (key.startsWith(m.prefix)) drop(key);
        scope.postMessage({ id: m.id, ok: true, done: true });
        return;
      case 'base':
        scope.postMessage({ id: m.id, ok: true, base: base(m.snapshot) });
        return;
    }
  } catch (err) {
    // Обычная ошибка разбора — плохие байты. Паника wasm (`unreachable`) ещё и портит модуль.
    const fatal = err instanceof WebAssembly.RuntimeError || (err instanceof Error && /unreachable/i.test(err.message));
    scope.postMessage({ id: m.id, ok: false, reason: 'bad', fatal } satisfies FromValidator);
  }
};

setInterval(() => {
  const now = performance.now();
  for (const [key, h] of docs) {
    if (now - h.used > (h.heavy ? HEAVY_IDLE_MS : IDLE_MS)) drop(key);
  }
}, 1000);
