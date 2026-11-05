// Сеанс журнала: ключи пачек одного файла упорядочены по времени открытия редактора
// и не совпадают у двух сеансов одной миллисекунды; число — целое в пределах
// Number.MAX_SAFE_INTEGER (так его принимает worker ввода-вывода).

import { expect, test } from '@playwright/test';
import type { CoreEvent } from '../src/wasm/ferrum_wasm';
import type { Editor } from '../src/ui/context';
import type { FileStore, JournalMark } from '../src/ui/file-store';
import { NEW_FILE_TAKEN } from '../src/core/file-list';
import {
  createSaver,
  FULL_IDLE_MS,
  FULL_MAX_MS,
  JOURNAL_MS,
  newSession,
  NOT_ON_SERVER,
  sessionAfter,
  SETTLE_MS,
  SERVER_REFUSED,
  SYNC_WAIT_MS,
  THUMB_MS,
  withTimeout,
  type SaverOptions,
} from '../src/ui/saver';

test('сеанс журнала: по времени, целый, безопасный', () => {
  const t = Date.UTC(2026, 9, 28, 12, 0, 0);
  expect(newSession(t, 0)).toBe(t * 1000);
  expect(newSession(t, 0.9999)).toBe(t * 1000 + 999);
  expect(newSession(t + 1, 0)).toBeGreaterThan(newSession(t, 0.9999));
  // До 2250 года число остаётся безопасным целым.
  const far = Date.UTC(2250, 0, 1);
  expect(Number.isSafeInteger(newSession(far, 0.9999))).toBe(true);
});

test('интервалы: журнал — секунды, полный файл — реже', () => {
  expect(JOURNAL_MS).toBe(2000);
  expect(FULL_IDLE_MS).toBeGreaterThan(JOURNAL_MS * 10);
  expect(FULL_MAX_MS).toBeGreaterThan(FULL_IDLE_MS);
});

test('сеанс журнала: часы ушли назад — номер всё равно больше оставшихся пачек', () => {
  const t = Date.UTC(2026, 9, 28, 12, 0, 0);
  // Журнала нет или он старше — сеанс по времени.
  expect(sessionAfter(null, t, 0)).toBe(t * 1000);
  expect(sessionAfter(t * 1000 - 5, t, 0)).toBe(t * 1000);
  // Пачки прежнего сеанса «из будущего»: новый сеанс — следующий за ними, иначе
  // запись файла (стирает `[id] … [id, сеанс, upTo]`) не стёрла бы их.
  const later = (t + 60_000) * 1000 + 7;
  expect(sessionAfter(later, t, 0.5)).toBe(later + 1);
});

// --- createSaver с поддельными ядром и хранилищем (без браузера) ---

type Listener = (e: CoreEvent) => void;

// Подменённые глобальные объекты возвращаются после каждого теста.
const g = globalThis as Record<string, unknown>;
const saved = ['window', 'document', 'performance', 'clearTimeout'].map(
  (k) => [k, Object.getOwnPropertyDescriptor(globalThis, k)] as const,
);
test.afterEach(() => {
  for (const [k, d] of saved) {
    if (d) Object.defineProperty(globalThis, k, d);
    else delete g[k];
  }
});

function harness(stored: boolean, journalSession?: number | null, fullHistory?: boolean, remote?: boolean, opts: SaverOptions = {}) {
  /** Поддельные часы: таймеры `window` и `performance.now()`. */
  let now = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  const windowEvents = new Map<string, (e: unknown) => void>();
  let seq = 0;
  g.window = {
    setTimeout: (fn: () => void, ms = 0) => (timers.set(++seq, { at: now + ms, fn }), seq),
    clearTimeout: (n: number) => timers.delete(n),
    addEventListener: (type: string, fn: (e: unknown) => void) => void windowEvents.set(type, fn),
    removeEventListener: (type: string) => void windowEvents.delete(type),
  };
  // В браузере `clearTimeout` и `window.clearTimeout` — одно; сохранитель зовёт и так.
  const realClear = clearTimeout;
  g.clearTimeout = (n: unknown) => (typeof n === 'number' ? void timers.delete(n) : realClear(n as NodeJS.Timeout));
  g.document = { visibilityState: 'visible', addEventListener() {}, removeEventListener() {} };
  Object.defineProperty(globalThis, 'performance', { value: { now: () => now }, configurable: true, writable: true });
  /** Часы вперёд на `ms`: срабатывают таймеры, чей срок настал, по порядку срока. */
  const advance = (ms: number) => {
    const end = now + ms;
    for (;;) {
      const due = [...timers].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      timers.delete(due[0]);
      now = due[1].at;
      due[1].fn();
    }
    now = end;
  };
  const listeners: Listener[] = [];
  const sent: { type: string }[] = [];
  const toasts: string[] = [];
  const ed = {
    core: { on: (l: Listener) => (listeners.push(l), () => void listeners.splice(listeners.indexOf(l) >>> 0, 1)) },
    store: { toast: (m: string) => void toasts.push(m), state: { stopped: false } },
    send: (c: { type: string }) => void sent.push(c),
  } as unknown as Editor;
  const marks: JournalMark[] = [];
  const writes: ({ session: number; upTo: number } | null)[] = [];
  let failAppend = false;
  let failWrite = false;
  let writeFailure: Error = new Error('недостаточно места в хранилище браузера');
  let hangAppend = false;
  let warning: string | undefined;
  const created: string[] = [];
  const thumbs: (ArrayBuffer | null)[] = [];
  const cuts: JournalMark[] = [];
  const files = {
    fullHistory,
    remote,
    putThumb: (_id: string, thumb: ArrayBuffer | null) => void thumbs.push(thumb),
    cutJournal: async (_id: string, mark: JournalMark) => void cuts.push(mark),
    createFrom: async (name: string) => (created.push(name), { id: 'новый' }),
    appendJournal: async (_id: string, mark: JournalMark) => {
      if (hangAppend) await new Promise(() => {});
      if (failAppend) throw new Error('нет места');
      marks.push(mark);
    },
    write: async (_id: string, _b: ArrayBuffer, _t: unknown, base: number, journal: { session: number; upTo: number } | null) => {
      writes.push(journal);
      if (failWrite) throw writeFailure;
      return { version: base + 1, ...(warning ? { warning } : {}) };
    },
  } as unknown as FileStore;
  const saver = createSaver(ed, files, 'f', 1, stored, { journalSession, ...opts });
  const emit = (e: CoreEvent) => listeners.forEach((l) => l(e));
  const tick = () => new Promise((r) => setTimeout(r, 0));
  return {
    saver,
    emit,
    tick,
    sent,
    toasts,
    marks,
    writes,
    timers,
    created,
    thumbs,
    cuts,
    advance,
    now: () => now,
    failAppends: (on: boolean) => void (failAppend = on),
    hangAppends: (on: boolean) => void (hangAppend = on),
    failWrites: (on: boolean, code?: string) => {
      failWrite = on;
      if (code) writeFailure = Object.assign(new Error(code), { code });
    },
    windowEvents,
    warnWrites: (text: string | undefined) => void (warning = text),
    stop: () => void ((ed.store as unknown as { state: { stopped: boolean } }).state.stopped = true),
    batch: (batch: number) => emit({ type: 'journal', batch, bytes: new Uint8Array([1, 2]), failed: false }),
    snapshot: (journal: number) =>
      emit({ type: 'snapshot', revision: 1, name: 'f', bytes: new Uint8Array(4), thumbnail: undefined, keepThumbnail: true, journal }),
  };
}

test('пачка не собралась (failed): flushJournal — false без зависания, правки — в следующей пачке', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  const flushed = h.saver.flushJournal();
  expect(h.sent.at(-1)?.type).toBe('flushJournal');
  h.emit({ type: 'journal', batch: 0, bytes: undefined, failed: true });
  expect(await flushed).toBe(false);
  expect(h.saver.pending(), 'правки ещё не в журнале').toBe(true);
  // Ядро не сдвинуло начало пачки: следующая удачная несёт те же правки — журнал полон
  // без полного файла.
  const again = h.saver.flushJournal();
  h.batch(1);
  expect(await again).toBe(true);
  expect(h.saver.pending()).toBe(false);
  expect(h.writes).toHaveLength(0);
});

test('пачки не собираются при непрерывных правках: полный файл не позже FULL_MAX_MS', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  const snapshotAt = () => h.sent.findIndex((c) => c.type === 'snapshot');
  // Правка каждую секунду; на каждую просьбу пачки ядро отвечает `failed`.
  while (snapshotAt() < 0 && h.now() < 2 * FULL_MAX_MS) {
    h.emit({ type: 'document', canUndo: true, canRedo: false, steps: 1 });
    const asked = h.sent.length;
    h.advance(1000);
    if (h.sent.slice(asked).some((c) => c.type === 'flushJournal')) {
      h.emit({ type: 'journal', batch: 0, bytes: undefined, failed: true });
    }
    await h.tick();
  }
  expect(snapshotAt(), 'полный файл запрошен').toBeGreaterThan(-1);
  expect(h.now()).toBeLessThanOrEqual(FULL_MAX_MS);
});

test('пачка — не позже JOURNAL_MS после первой правки, правки подряд её не откладывают', () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.advance(1500);
  h.emit({ type: 'document', canUndo: true, canRedo: false, steps: 1 });
  h.advance(400);
  h.emit({ type: 'document', canUndo: true, canRedo: false, steps: 2 });
  expect(h.sent.filter((c) => c.type === 'flushJournal')).toHaveLength(0);
  h.advance(100);
  expect(h.sent.filter((c) => c.type === 'flushJournal')).toHaveLength(1);
});

test('хранилище полно (пачка и файл не пишутся): правки каждые 100 мс не гонят снимки подряд', async () => {
  const h = harness(true);
  h.failAppends(true);
  h.failWrites(true);
  // Ядро отвечает сразу: на просьбу пачки — пачкой, на снимок — снимком.
  let answered = 0;
  let batch = 0;
  const answer = () => {
    for (; answered < h.sent.length; answered++) {
      const c = h.sent[answered];
      if (c.type === 'flushJournal') h.batch(++batch);
      if (c.type === 'snapshot') h.snapshot(batch);
    }
  };
  h.emit({ type: 'modified', modified: true });
  const end = FULL_MAX_MS + 30_000;
  while (h.now() < end) {
    h.emit({ type: 'document', canUndo: true, canRedo: false, steps: 1 });
    h.advance(100);
    answer();
    for (let i = 0; i < 20; i++) await Promise.resolve();
    answer();
  }
  const snaps = h.sent.filter((c) => c.type === 'snapshot').length;
  // Журнал неполон: первая попытка — к 5 минутам; после неудачи следующая — не раньше
  // чем через FULL_IDLE_MS (у 330-й секунды — граница окна), а не на каждую правку.
  expect(snaps).toBeGreaterThanOrEqual(1);
  expect(snaps).toBeLessThanOrEqual(2);
  expect(h.toasts.filter((t) => t.startsWith('Не удалось сохранить файл'))).toHaveLength(1);
});

test('5-минутная проверка не запускает второй снимок, пока идёт запись', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.advance(FULL_MAX_MS + 1);
  // Запись по таймеру простоя уже идёт; пачка перед её снимком приходит после 5 минут.
  const snaps = () => h.sent.filter((c) => c.type === 'snapshot').length;
  expect(snaps()).toBe(1);
  h.batch(1);
  h.snapshot(1);
  await h.tick();
  await h.tick();
  expect(snaps()).toBe(1);
});

test('ядро остановлено: flushJournal не говорит «всё в журнале» при потерянной пачке', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.failAppends(true);
  const first = h.saver.flushJournal();
  h.batch(1);
  expect(await first).toBe(false);
  h.stop();
  expect(await h.saver.flushJournal()).toBe(false);
});

test('отказ записи пачки: уведомление одно, правки — полным файлом', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.failAppends(true);
  const first = h.saver.flushJournal();
  h.batch(0);
  expect(await first).toBe(false);
  const second = h.saver.flushJournal();
  h.batch(1);
  expect(await second).toBe(false);
  expect(h.toasts.filter((t) => t.startsWith('Не удалось записать журнал изменений: нет места'))).toHaveLength(1);
  // Запись снова удалась, но первые пачки потеряны — до полного файла правки не в журнале.
  h.failAppends(false);
  const third = h.saver.flushJournal();
  h.batch(2);
  expect(await third).toBe(false);
  expect(h.saver.pending()).toBe(true);
});

test('ядро остановлено: ожидающие flushJournal — false', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  const flushed = h.saver.flushJournal();
  h.emit({ type: 'error', message: 'Ядро остановлено: сбой' });
  expect(await flushed).toBe(false);
});

test('перезапуск после сбоя ядра: ожидающие flushJournal — false, снятый сохранитель молчит', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  const flushed = h.saver.flushJournal();
  const saving = h.saver.save();
  // Ядро остановлено, редактор снят (перезапуск): ответа ядра не будет никому.
  h.emit({ type: 'error', message: 'Ядро остановлено: паника в ядре: тестовая паника ядра' });
  h.saver.dispose();
  expect(await flushed).toBe(false);
  expect(await saving).toBe(false);
  // Ожидающий без события остановки (снят раньше ответа ядра) — тоже false, без зависания.
  const k = harness(true);
  k.emit({ type: 'modified', modified: true });
  const late = k.saver.flushJournal();
  k.saver.dispose();
  expect(await late).toBe(false);
  // Снятый сохранитель не слушает ядро и не держит таймеров: пачки и правки нового
  // ядра (тот же файл) пишет только новый сохранитель.
  const sent = k.sent.length;
  k.emit({ type: 'modified', modified: true });
  k.batch(5);
  k.emit({ type: 'saveRequested' });
  await k.tick();
  k.advance(FULL_MAX_MS * 2);
  expect(k.timers.size).toBe(0);
  expect(k.sent.length).toBe(sent);
  expect(k.marks).toEqual([]);
  expect(k.writes).toEqual([]);
});

test('сбой ядра, пока пишется пачка: settled ждёт записи, pending — по её итогу', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  // Пачка отдана ядром, запись в хранилище ещё идёт — и не удастся.
  h.failAppends(true);
  h.batch(0);
  h.emit({ type: 'error', message: 'Ядро остановлено: паника в ядре: тестовая паника ядра' });
  // Сразу после сбоя правки выглядят «в журнале» — сообщение было бы ложным.
  expect(h.saver.pending()).toBe(false);
  expect(await h.saver.settled()).toBe(true);
  expect(h.saver.pending()).toBe(true);
  // Удачная запись — правки в журнале.
  const k = harness(true);
  k.emit({ type: 'modified', modified: true });
  k.batch(0);
  await k.saver.settled();
  expect(k.saver.pending()).toBe(false);
});

test('хранилище не отвечает: settled — false через SETTLE_MS, не вечное ожидание', async () => {
  expect(SETTLE_MS).toBeGreaterThanOrEqual(2000);
  expect(SETTLE_MS).toBeLessThanOrEqual(3000);
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.hangAppends(true);
  h.batch(0);
  h.emit({ type: 'error', message: 'Ядро остановлено: паника в ядре: тестовая паника ядра' });
  let result: boolean | undefined;
  void h.saver.settled().then((ok) => (result = ok));
  await h.tick();
  h.advance(SETTLE_MS - 1);
  await h.tick();
  expect(result).toBeUndefined();
  h.advance(1);
  await h.tick();
  expect(result).toBe(false);
});

test('сеанс сохранителя — после наибольшего сеанса журнала файла', async () => {
  const future = (Date.now() + 3_600_000) * 1000 + 3;
  const h = harness(true, future);
  h.emit({ type: 'modified', modified: true });
  const flushed = h.saver.flushJournal();
  h.batch(0);
  expect(await flushed).toBe(true);
  expect(h.marks).toEqual([{ session: future + 1, batch: 0 }]);
  const now = Date.now() * 1000;
  const p = harness(true, 5);
  p.emit({ type: 'modified', modified: true });
  const ok = p.saver.flushJournal();
  p.batch(0);
  expect(await ok).toBe(true);
  expect(p.marks[0].session).toBeGreaterThanOrEqual(now);
});

test('withTimeout: ответ раньше предела — он, и таймер снят; позже — запасное значение', async () => {
  const h = harness(true);
  const before = h.timers.size;
  let done!: (v: boolean) => void;
  const fast = withTimeout(new Promise<boolean>((r) => (done = r)), SETTLE_MS, false);
  expect(h.timers.size).toBe(before + 1);
  done(true);
  expect(await fast).toBe(true);
  expect(h.timers.size).toBe(before);
  let result: boolean | undefined;
  void withTimeout(new Promise<boolean>(() => {}), SETTLE_MS, false).then((v) => (result = v));
  h.advance(SETTLE_MS - 1);
  await h.tick();
  expect(result).toBeUndefined();
  h.advance(1);
  await h.tick();
  expect(result).toBe(false);
});

test('запись удалась с предупреждением (превью не записано): одно уведомление, запись не ошибка', async () => {
  const h = harness(true);
  h.warnWrites('Превью файла не записано: нет места');
  for (let i = 1; i <= 3; i++) {
    h.emit({ type: 'modified', modified: true });
    const saved = h.saver.save();
    await h.tick();
    h.snapshot(0);
    expect(await saved).toBe(true);
  }
  expect(h.writes).toHaveLength(3);
  expect(h.toasts).toEqual(['Превью файла не записано: нет места']);
  expect(h.saver.failure()).toBeNull();
  // Удачная запись без предупреждения — следующее предупреждение снова показывается.
  h.warnWrites(undefined);
  h.emit({ type: 'modified', modified: true });
  let saved = h.saver.save();
  await h.tick();
  h.snapshot(0);
  await saved;
  h.warnWrites('Превью файла не записано: нет места');
  h.emit({ type: 'modified', modified: true });
  saved = h.saver.save();
  await h.tick();
  h.snapshot(0);
  await saved;
  expect(h.toasts).toHaveLength(2);
});

test('снимок полный только там, где хранилище этого просит: файл на сервере', () => {
  const asked = (fullHistory?: boolean) => {
    const h = harness(true, null, fullHistory);
    h.emit({ type: 'modified', modified: true });
    void h.saver.save();
    return h.sent.find((c) => c.type === 'snapshot');
  };
  expect(asked()).toEqual({ type: 'snapshot', thumbnail: true });
  expect(asked(true)).toEqual({ type: 'snapshot', thumbnail: true, full: true });
});

test('`CONFLICT` от сервера: причина для вопроса при уходе, признак не снят, поколение прежнее', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  h.failWrites(true);
  const saving = h.saver.save();
  h.snapshot(0);
  expect(await saving).toBe(false);
  expect(h.saver.failure()).toContain('Не удалось сохранить файл');
  expect(h.saver.version()).toBe(1);
  expect(h.sent.some((c) => c.type === 'markSaved')).toBe(false);
});

const edit = (h: ReturnType<typeof harness>) => h.emit({ type: 'modified', modified: true });
/** Снимок на просьбу сохранителя: ядро отвечает сразу. */
async function answerSnapshot(h: ReturnType<typeof harness>) {
  await h.tick();
  h.snapshot(0);
  await h.tick();
  await h.tick();
}

test('запись через сеть: неудачная повторяется сама, отказ, который не исправить, — нет (T6-I4)', async () => {
  const h = harness(true, null, true, true);
  edit(h);
  h.failWrites(true, 'NETWORK');
  void h.saver.save();
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(1);
  h.failWrites(false);
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(2);
  expect(h.saver.failure()).toBeNull();

  const c = harness(true, null, true, true);
  edit(c);
  c.failWrites(true, 'CONFLICT');
  void c.saver.save();
  await answerSnapshot(c);
  c.failWrites(false);
  c.advance(FULL_MAX_MS * 2);
  await answerSnapshot(c);
  expect(c.writes).toHaveLength(1);
});

test('автоповтор через сеть: пауза растёт до десяти минут, снимков за ночь — десятки (R-I1)', async () => {
  const h = harness(true, null, true, true);
  edit(h);
  h.failWrites(true, 'NETWORK');
  const snapshots = () => h.sent.filter((c) => c.type === 'snapshot').length;
  void h.saver.save();
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(1);
  // Каждая следующая попытка — только к своему сроку: на шаг раньше снимка нет.
  for (const [i, minutes] of [0.5, 1, 2, 5, 10, 10, 10].entries()) {
    const ms = minutes * 60_000;
    h.advance(ms - 1);
    await h.tick();
    expect(snapshots(), `до срока попытки ${i + 2}`).toBe(i + 1);
    h.advance(1);
    await answerSnapshot(h);
    expect(snapshots(), `попытка ${i + 2}`).toBe(i + 2);
    expect(h.writes).toHaveLength(i + 2);
  }
  // Ночь без сети: десять часов, снимок не чаще раза в десять минут.
  const before = snapshots();
  for (let i = 0; i < 60; i++) {
    h.advance(10 * 60_000);
    await answerSnapshot(h);
  }
  expect(snapshots() - before).toBeLessThanOrEqual(60);
});

test('автоповтор: правка и удача начинают паузы сначала, `HASH_MISMATCH` не повторяется', async () => {
  const h = harness(true, null, true, true);
  edit(h);
  h.failWrites(true, 'NETWORK');
  void h.saver.save();
  await answerSnapshot(h);
  for (const minutes of [0.5, 1, 2]) {
    h.advance(minutes * 60_000);
    await answerSnapshot(h);
  }
  expect(h.writes).toHaveLength(4);
  // Правка: снова 30 с, затем снова 30 с первой ступени, потом уже минута.
  h.emit({ type: 'document', canUndo: true, canRedo: false, steps: 1 });
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(5);
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(6);
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(6);
  // Удача сбрасывает ступени: следующий сбой снова ждёт 30 с.
  h.failWrites(false);
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(7);
  expect(h.saver.failure()).toBeNull();
  // Без новой правки: «изменён» ещё не снят, запись по просьбе человека снова не удалась.
  h.failWrites(true, 'NETWORK');
  void h.saver.save();
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(8);
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(9);

  for (const code of ['HASH_MISMATCH', 'TOO_MANY_IMAGES']) {
    const c = harness(true, null, true, true);
    edit(c);
    c.failWrites(true, code);
    void c.saver.save();
    await answerSnapshot(c);
    c.advance(FULL_MAX_MS * 2);
    await answerSnapshot(c);
    expect(c.writes, code).toHaveLength(1);
  }
});

test('снятый сохранитель: запись в пути не заводит новую попытку', async () => {
  const h = harness(true, null, true, true);
  edit(h);
  h.failWrites(true, 'NETWORK');
  void h.saver.save();
  await h.tick();
  h.snapshot(0);
  h.saver.dispose();
  await h.tick();
  await h.tick();
  expect(h.timers.size).toBe(0);
});

test('закрытие вкладки спрашивает, пока пачка журнала дописывается (R-I2)', async () => {
  const h = harness(true, null, true, true);
  const asks = () => {
    let asked = false;
    h.windowEvents.get('beforeunload')?.({ preventDefault: () => (asked = true), returnValue: undefined });
    return asked;
  };
  edit(h);
  h.hangAppends(true);
  void h.saver.flushJournal();
  h.batch(1);
  // Ядро отдало пачку: признак снят, а запись в хранилище ещё идёт.
  expect(h.saver.pending()).toBe(false);
  expect(asks()).toBe(true);

  const ok = harness(true, null, true, true);
  const okAsks = () => {
    let asked = false;
    ok.windowEvents.get('beforeunload')?.({ preventDefault: () => (asked = true), returnValue: undefined });
    return asked;
  };
  edit(ok);
  void ok.saver.flushJournal();
  ok.batch(1);
  await ok.tick();
  expect(okAsks(), 'пачка дописана — спрашивать не о чем').toBe(false);
  const bad = harness(true, null, true, true);
  edit(bad);
  bad.failAppends(true);
  void bad.saver.flushJournal();
  bad.batch(1);
  await bad.tick();
  let asked = false;
  bad.windowEvents.get('beforeunload')?.({ preventDefault: () => (asked = true), returnValue: undefined });
  expect(asked, 'потерянная пачка — правки только в памяти').toBe(true);
});

test('новый файл, а записал другой человек: честная причина, запись не повторяется, правки — новым файлом (C-1)', async () => {
  const diverged: number[] = [];
  const h = harness(false, null, true, true, { onDiverged: () => diverged.push(1) });
  edit(h);
  h.failWrites(true, 'CONFLICT');
  void h.saver.save();
  await answerSnapshot(h);
  expect(diverged).toHaveLength(1);
  expect(h.saver.failure()).toBe(NEW_FILE_TAKEN);
  expect(h.toasts).toEqual([NEW_FILE_TAKEN]);
  expect(h.saver.failure()).toContain('пропадут');
  expect(h.saver.failure()).not.toContain('правки сольются');
  // Ни автоповтора, ни снимков при уходе: отказ не изменится.
  const snapshots = h.sent.filter((c) => c.type === 'snapshot').length;
  h.advance(FULL_MAX_MS * 2);
  await h.tick();
  expect(await h.saver.save()).toBe(false);
  expect(h.sent.filter((c) => c.type === 'snapshot')).toHaveLength(snapshots);
  expect(h.saver.pending()).toBe(true);
  // Новым файлом: снимок идёт туда, уходить больше не о чем спрашивать.
  const made = h.saver.saveAsNew('Копия');
  h.snapshot(0);
  expect(await made).toMatchObject({ id: 'новый' });
  expect(h.created).toEqual(['Копия']);
  expect(h.saver.pending()).toBe(false);
  expect(await h.saver.save()).toBe(true);
});

test('записанный файл при `CONFLICT` остаётся с журналом: правки сольются, а не спасаются', async () => {
  const diverged: number[] = [];
  const h = harness(true, null, true, true, { onDiverged: () => diverged.push(1) });
  edit(h);
  h.failWrites(true, 'CONFLICT');
  void h.saver.save();
  await answerSnapshot(h);
  expect(diverged).toEqual([]);
  expect(h.saver.failure()).toContain('Не удалось сохранить файл');
});

test('запись без сети (автономная сборка): сама не повторяется, закрытие вкладки не спрашивает', async () => {
  const h = harness(false);
  expect(h.windowEvents.has('beforeunload')).toBe(false);
  edit(h);
  h.failWrites(true);
  void h.saver.save();
  await answerSnapshot(h);
  h.failWrites(false);
  h.advance(FULL_IDLE_MS);
  await answerSnapshot(h);
  expect(h.writes).toHaveLength(1);
});

test('закрытие вкладки: спрашивает, только пока правки есть лишь в памяти; согласие снимает вопрос (T6-I4)', async () => {
  const ask = (h: ReturnType<typeof harness>) => {
    let asked = false;
    h.windowEvents.get('beforeunload')?.({ preventDefault: () => (asked = true), returnValue: undefined });
    return asked;
  };
  const fresh = harness(false, null, true, true);
  expect(ask(fresh)).toBe(false);
  edit(fresh);
  expect(ask(fresh)).toBe(true);
  fresh.saver.release();
  expect(ask(fresh)).toBe(false);

  // У записанного файла правки в журнале: вопрос не нужен.
  const stored = harness(true, null, true, true);
  edit(stored);
  stored.batch(1);
  await stored.tick();
  expect(stored.saver.pending()).toBe(false);
  expect(ask(stored)).toBe(false);
});

// --- Журнал сервера: файл целиком пишется только первый раз ---

const synced = (stored: boolean, opts: SaverOptions = {}) => harness(stored, null, true, true, { sync: true, ...opts });

test('журнал сервера: правки идут журналом, полный файл не пишется ни в простое, ни со временем', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.advance(FULL_MAX_MS * 3);
  await h.tick();
  expect(h.sent.some((c) => c.type === 'flushJournal')).toBe(true);
  expect(h.sent.some((c) => c.type === 'snapshot')).toBe(false);
  expect(h.writes).toHaveLength(0);
});

test('журнал сервера: новый файл записывается сразу, без правок; потом можно подключаться', async () => {
  let stored = 0;
  const h = harness(false, null, true, true, { sync: true, onStored: () => stored++ });
  await h.tick();
  expect(h.sent.find((c) => c.type === 'snapshot')).toBeTruthy();
  h.snapshot(0);
  await h.tick();
  await h.tick();
  expect(h.writes).toHaveLength(1);
  expect(stored).toBe(1);
  expect(h.saver.stored()).toBe(true);
});

test('журнал сервера: читатель первую запись не делает', async () => {
  const h = harness(false, null, true, true, { sync: true, viewer: true });
  await h.tick();
  expect(h.sent.some((c) => c.type === 'snapshot')).toBe(false);
});

test('журнал сервера: первую запись опередили, своих правок нет — файл открывается заново, а не теряет чужое', async () => {
  let taken = 0;
  const h = harness(false, null, true, true, { sync: true, onTaken: () => taken++ });
  h.failWrites(true, 'CONFLICT');
  await h.tick();
  h.snapshot(0);
  await h.tick();
  await h.tick();
  expect(taken).toBe(1);
});

test('журнал сервера: первую запись опередили, а правки уже есть — честный вопрос, правки не пропадают молча', async () => {
  let taken = 0;
  let diverged = 0;
  const h = harness(false, null, true, true, { sync: true, onTaken: () => taken++, onDiverged: () => diverged++ });
  h.failWrites(true, 'CONFLICT');
  h.emit({ type: 'modified', modified: true });
  await h.tick();
  h.snapshot(0);
  await h.tick();
  await h.tick();
  expect(taken).toBe(0);
  expect(diverged).toBe(1);
});

test('журнал сервера: «Сохранить» ждёт сервера; подтвердил — записано', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.emit({ type: 'syncState', unacked: true });
  const saved = h.saver.save();
  await h.tick();
  h.emit({ type: 'journal', batch: 1, bytes: new Uint8Array([1]), failed: false });
  await h.tick();
  await h.tick();
  h.emit({ type: 'syncState', unacked: false });
  expect(await saved).toBe(true);
  expect(h.saver.failure()).toBeNull();
});

test('журнал сервера: сервер не подтвердил за срок — уход спросит, причина не пугает: правки в браузере', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.emit({ type: 'syncState', unacked: true });
  const saved = h.saver.save();
  await h.tick();
  h.emit({ type: 'journal', batch: 1, bytes: new Uint8Array([1]), failed: false });
  await h.tick();
  await h.tick();
  h.advance(SYNC_WAIT_MS);
  expect(await saved).toBe(false);
  expect(h.saver.failure()).toBe(NOT_ON_SERVER);
  // Позже подтвердил: причина снята.
  h.emit({ type: 'syncState', unacked: false });
  expect(h.saver.failure()).toBeNull();
});

test('журнал сервера: после пяти минут работы превью не рендерится по каждой пачке (T9-I3)', async () => {
  const h = synced(true);
  h.advance(FULL_MAX_MS + 1);
  h.emit({ type: 'modified', modified: true });
  for (let i = 1; i <= 5; i++) {
    h.emit({ type: 'document', canUndo: true, canRedo: false, steps: i });
    h.advance(JOURNAL_MS);
    h.batch(i);
    await h.tick();
    await h.tick();
  }
  // Правки шли 10 с: превью ещё не положено (раз в минуту), пачка журнала — одна на каждую.
  expect(h.sent.filter((c) => c.type === 'thumbnail')).toHaveLength(0);
  expect(h.sent.filter((c) => c.type === 'flushJournal')).toHaveLength(5);
  h.advance(THUMB_MS);
  expect(h.sent.filter((c) => c.type === 'thumbnail')).toHaveLength(1);
});

test('журнал сервера: сервер правки не примет — уход спрашивает сразу и говорит правду (T9-I1)', async () => {
  let why: string | null = null;
  const h = synced(true, { refused: () => why });
  h.emit({ type: 'modified', modified: true });
  h.emit({ type: 'syncState', unacked: true });
  why = SERVER_REFUSED;
  const saved = h.saver.save();
  await h.tick();
  h.emit({ type: 'journal', batch: 1, bytes: new Uint8Array([1]), failed: false });
  await h.tick();
  await h.tick();
  // Не ждёт срока: исход известен.
  expect(await saved).toBe(false);
  expect(h.saver.failure()).toBe(SERVER_REFUSED);
  expect(h.saver.failure()).not.toBe(NOT_ON_SERVER);
});

test('журнал сервера: соединение не запустилось — «Сохранить» не отвечает «записано» (T9-M4)', async () => {
  const h = synced(true, { refused: () => SERVER_REFUSED });
  // Ядро без синхронизации: «изменён», но `syncState` не приходит.
  h.emit({ type: 'modified', modified: true });
  const saved = h.saver.save();
  await h.tick();
  h.batch(1);
  await h.tick();
  await h.tick();
  expect(await saved).toBe(false);
  expect(h.saver.failure()).toBe(SERVER_REFUSED);
});

test('журнал сервера: пачка не записалась в браузере, а сервер всё подтвердил — файл сохранён (T9-M1)', async () => {
  const h = synced(true);
  h.emit({ type: 'modified', modified: true });
  h.emit({ type: 'syncState', unacked: true });
  h.failAppends(true);
  h.advance(JOURNAL_MS);
  h.batch(1);
  await h.tick();
  await h.tick();
  expect(h.saver.pending()).toBe(true);
  // Полных записей нет: потерянная пачка не заводит «Сохранить» по таймеру (и превью с ним).
  const flushes = h.sent.filter((c) => c.type === 'flushJournal').length;
  h.advance(FULL_IDLE_MS);
  await h.tick();
  expect(h.sent.filter((c) => c.type === 'flushJournal')).toHaveLength(flushes);
  expect(h.sent.some((c) => c.type === 'snapshot' || c.type === 'thumbnail')).toBe(false);
  h.failAppends(false);
  h.emit({ type: 'syncState', unacked: false });
  h.emit({ type: 'modified', modified: false });
  expect(h.saver.pending()).toBe(false);
  const saved = h.saver.save();
  await h.tick();
  h.batch(2);
  await h.tick();
  await h.tick();
  expect(await saved).toBe(true);
  expect(h.saver.failure()).toBeNull();
});

test('журнал сервера: подтверждение снимает журнал браузера отметкой своего сеанса', async () => {
  const h = synced(true);
  h.emit({ type: 'journalCovered', batch: 3 });
  await h.tick();
  expect(h.cuts.map((m) => m.batch)).toEqual([3]);
});
