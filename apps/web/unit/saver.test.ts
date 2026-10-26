// Сеанс журнала: ключи пачек одного файла упорядочены по времени открытия редактора
// и не совпадают у двух сеансов одной миллисекунды; число — целое в пределах
// Number.MAX_SAFE_INTEGER (так его принимает worker ввода-вывода).

import { expect, test } from '@playwright/test';
import type { CoreEvent } from '../src/wasm/ferrum_wasm';
import type { Editor } from '../src/ui/context';
import type { FileStore, JournalMark } from '../src/ui/file-store';
import { createSaver, FULL_IDLE_MS, FULL_MAX_MS, JOURNAL_MS, newSession, sessionAfter } from '../src/ui/saver';

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

function harness(stored: boolean, journalSession?: number | null) {
  const timers = new Map<number, () => void>();
  let seq = 0;
  const g = globalThis as Record<string, unknown>;
  g.window = {
    setTimeout: (fn: () => void) => (timers.set(++seq, fn), seq),
    clearTimeout: (n: number) => timers.delete(n),
    addEventListener() {},
    removeEventListener() {},
  };
  g.document = { visibilityState: 'visible', addEventListener() {}, removeEventListener() {} };
  const listeners: Listener[] = [];
  const sent: { type: string }[] = [];
  const toasts: string[] = [];
  const ed = {
    core: { on: (l: Listener) => (listeners.push(l), () => {}) },
    store: { toast: (m: string) => void toasts.push(m), state: { stopped: false } },
    send: (c: { type: string }) => void sent.push(c),
  } as unknown as Editor;
  const marks: JournalMark[] = [];
  const writes: ({ session: number; upTo: number } | null)[] = [];
  let failAppend = false;
  const files = {
    appendJournal: async (_id: string, mark: JournalMark) => {
      if (failAppend) throw new Error('нет места');
      marks.push(mark);
    },
    write: async (_id: string, _b: ArrayBuffer, _t: unknown, base: number, journal: { session: number; upTo: number } | null) => {
      writes.push(journal);
      return { version: base + 1 };
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
    failAppends: (on: boolean) => void (failAppend = on),
    batch: (batch: number) => emit({ type: 'journal', batch, bytes: new Uint8Array([1, 2]), failed: false }),
    snapshot: (journal: number) =>
      emit({ type: 'snapshot', revision: 1, name: 'f', bytes: new Uint8Array(4), thumbnail: undefined, keepThumbnail: true, journal }),
  };
}

test('пачка не собралась (failed): flushJournal — false без зависания, правки — полным файлом', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  const flushed = h.saver.flushJournal();
  expect(h.sent.at(-1)?.type).toBe('flushJournal');
  h.emit({ type: 'journal', batch: 0, bytes: undefined, failed: true });
  expect(await flushed).toBe(false);
  expect(h.saver.pending(), 'правки только в памяти').toBe(true);
  // Следующая пачка удалась, но правок из несобранной в журнале нет: всё ещё не в журнале.
  h.emit({ type: 'document', canUndo: true, canRedo: false, steps: 1 });
  const again = h.saver.flushJournal();
  h.batch(1);
  expect(await again).toBe(false);
  expect(h.saver.pending()).toBe(true);
  // Полный файл в простое забирает их: после записи журнал снова полон.
  const saved = h.saver.save();
  expect(h.sent.at(-1)?.type).toBe('snapshot');
  h.batch(2);
  h.snapshot(3);
  expect(await saved).toBe(true);
  expect(h.writes.at(-1)?.upTo).toBe(3);
  expect(h.saver.pending()).toBe(false);
});

test('несобранная пачка после снимка не снимается записью файла', async () => {
  const h = harness(true);
  h.emit({ type: 'modified', modified: true });
  const saved = h.saver.save();
  // Пачка перед снимком дописана — правок вне журнала нет.
  h.batch(1);
  h.snapshot(1);
  // Правка после снимка, её пачка не собралась, пока файл пишется.
  h.emit({ type: 'journal', batch: 1, bytes: undefined, failed: true });
  expect(await saved).toBe(true);
  expect(h.writes.at(-1)?.upTo).toBe(1);
  expect(h.saver.pending()).toBe(true);
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
