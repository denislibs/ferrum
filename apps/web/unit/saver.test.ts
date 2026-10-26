// Сеанс журнала: ключи пачек одного файла упорядочены по времени открытия редактора
// и не совпадают у двух сеансов одной миллисекунды; число — целое в пределах
// Number.MAX_SAFE_INTEGER (так его принимает worker ввода-вывода).

import { expect, test } from '@playwright/test';
import type { CoreEvent } from '../src/wasm/ferrum_wasm';
import type { Editor } from '../src/ui/context';
import type { FileStore, JournalMark } from '../src/ui/file-store';
import { createSaver, FULL_IDLE_MS, FULL_MAX_MS, JOURNAL_MS, newSession, sessionAfter, SETTLE_MS, withTimeout } from '../src/ui/saver';

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
const saved = ['window', 'document', 'performance', 'clearTimeout'].map((k) => [k, Object.getOwnPropertyDescriptor(globalThis, k)] as const);
test.afterEach(() => {
  for (const [k, d] of saved) {
    if (d) Object.defineProperty(globalThis, k, d);
    else delete g[k];
  }
});

function harness(stored: boolean, journalSession?: number | null) {
  /** Поддельные часы: таймеры `window` и `performance.now()`. */
  let now = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  let seq = 0;
  g.window = {
    setTimeout: (fn: () => void, ms = 0) => (timers.set(++seq, { at: now + ms, fn }), seq),
    clearTimeout: (n: number) => timers.delete(n),
    addEventListener() {},
    removeEventListener() {},
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
  let hangAppend = false;
  let warning: string | undefined;
  const files = {
    appendJournal: async (_id: string, mark: JournalMark) => {
      if (hangAppend) await new Promise(() => {});
      if (failAppend) throw new Error('нет места');
      marks.push(mark);
    },
    write: async (_id: string, _b: ArrayBuffer, _t: unknown, base: number, journal: { session: number; upTo: number } | null) => {
      writes.push(journal);
      if (failWrite) throw new Error('недостаточно места в хранилище браузера');
      return { version: base + 1, ...(warning ? { warning } : {}) };
    },
  } as unknown as FileStore;
  const saver = createSaver(ed, files, 'f', 1, stored, { journalSession });
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
    advance,
    now: () => now,
    failAppends: (on: boolean) => void (failAppend = on),
    hangAppends: (on: boolean) => void (hangAppend = on),
    failWrites: (on: boolean) => void (failWrite = on),
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
