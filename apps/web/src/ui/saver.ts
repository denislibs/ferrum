// Сохранение открытого файла в хранилище (этап 6): журнал изменений и полный файл.
// - Журнал: не позже чем через `JOURNAL_MS` после правки ядро отдаёт пачку
//   обновлений Loro (`flushJournal` → событие `journal`, миллисекунды работы ядра),
//   она дописывается в журнал файла. Так правки переживают сбой ядра и закрытие
//   вкладки: при следующем открытии журнал применяется поверх файла.
// - Полный файл (снимок `snapshot`: на 100 000 узлах ≈ 0,45 с работы ядра) — в
//   простое: через `FULL_IDLE_MS` после последней правки и не реже `FULL_MAX_MS` при
//   работе без пауз; по Ctrl/Cmd+S; при уходе на экран файлов или к другому файлу
//   (уход ждёт записи). Перед снимком ядро отдаёт пачку с правками до него — она
//   дописывается раньше, чем пишется файл, а запись файла стирает пачки до номера
//   снимка в той же транзакции.
// - Новый файл без байтов в хранилище: журнал не к чему применить, поэтому до первой
//   записи он сохраняется полным файлом через `AUTOSAVE_MS` после правки.
// - Вкладку скрыли — сразу пачка журнала; `pagehide` — пачка и попытка полного файла
//   (без гарантии: вкладка может закрыться раньше записи).
// «Изменён» значит «не в файле»: признак снимает ядро (`markSaved`) только после
// записи полного файла и только если с момента снимка правок не было. Ошибка записи
// файла или журнала — одно уведомление, пока запись снова не удастся; документ
// остаётся открытым. Запись несёт поколение файла, прочитанное при открытии: файл
// записали в другой вкладке — отказ, а не тихая перезапись.
// Пачка, которую ядро не собрало (`journal` с `failed`), не потеряна: начало журнала
// ядро не сдвинуло, те же правки уйдут следующей удачной пачкой (до неё
// `flushJournal` отвечает `false`, `pending` — `true`). Пачку, которую не удалось
// дописать в хранилище, ядро уже отдало: её правки — только в памяти, пока их не
// заберёт полный файл. Пока журнал неполон (пачка не собрана или потеряна), правки
// не отодвигают полный файл дальше `FULL_MAX_MS` от прошлого. После неудачной
// полной записи следующая сама по себе — не раньше чем через `FULL_IDLE_MS`.

import type { CoreEvent } from '../wasm/ferrum_wasm';
import type { Editor } from './context';
import type { FileStore } from './file-store';
import { ownBuffer } from './file-store';

/** Новый файл без байтов в хранилище: полный файл через столько после правки. */
export const AUTOSAVE_MS = 2000;
/** Пачка журнала — не позже чем через столько после правки. */
export const JOURNAL_MS = 2000;
/** Полный файл — через столько после последней правки… */
export const FULL_IDLE_MS = 30_000;
/** …и не реже этого при правках без пауз. */
export const FULL_MAX_MS = 300_000;
/** После сбоя ядра запись последних пачек ждём не дольше этого: хранилище не
 * отвечает — перезапуск всё равно идёт, а правки считаются потерянными. */
export const SETTLE_MS = 2500;

export interface SaverOptions {
  /** Тесты: интервалы журнала и полного файла. */
  journalMs?: number;
  fullIdleMs?: number;
  /** Наибольший сеанс среди пачек журнала файла при открытии (`null` — пачек нет):
   * сеанс этого редактора — после него, даже если часы ушли назад. */
  journalSession?: number | null;
}

export interface Saver {
  /** Сохранить полный файл, если документ изменён; `true` — записано (или нечего). */
  save(): Promise<boolean>;
  /** Отдать журналу правки сейчас и дождаться записи пачки; `true` — всё в журнале. */
  flushJournal(): Promise<boolean>;
  /** Есть правки, которых нет ни в журнале, ни в файле (сообщение после сбоя ядра). */
  pending(): boolean;
  /** Отданные ядром пачки дописаны (или не удались): после сбоя ядра `pending` —
   * только после этого, иначе неудачная запись последней пачки прошла бы как «сохранено».
   * `false` — хранилище не ответило за `SETTLE_MS`: исход пачек неизвестен. */
  settled(): Promise<boolean>;
  /** Почему не удалась последняя запись (для вопроса при уходе); `null` — удалась. */
  failure(): string | null;
  dispose(): void;
}

/** Сеанс редактора для ключей журнала: время открытия в мс и три случайных разряда —
 * сеансы одного файла упорядочены по времени и не совпадают. */
export function newSession(now = Date.now(), random = Math.random()): number {
  return now * 1000 + Math.min(999, Math.floor(random * 1000));
}

/** Сеанс после пачек, оставшихся в журнале файла (`last` — наибольший их сеанс):
 * часы ушли назад — новый сеанс всё равно больше. Иначе запись файла, стирающая
 * пачки `[id] … [id, сеанс, upTo]`, не стёрла бы оставшиеся, а при открытии они шли
 * бы после новых. */
export function sessionAfter(last: number | null, now = Date.now(), random = Math.random()): number {
  return Math.max(newSession(now, random), last === null ? 0 : last + 1);
}

type Snapshot = CoreEvent & { type: 'snapshot' };
type Journal = CoreEvent & { type: 'journal' };

/** `base` — поколение файла (`FileMeta.version`) на момент открытия; `stored` — у
 * файла есть байты в хранилище (журнал есть к чему применить). */
export function createSaver(
  ed: Editor,
  files: FileStore,
  id: string,
  base: number,
  stored: boolean,
  opts: SaverOptions = {},
): Saver {
  const journalMs = opts.journalMs ?? JOURNAL_MS;
  const fullIdleMs = opts.fullIdleMs ?? FULL_IDLE_MS;
  const session = sessionAfter(opts.journalSession ?? null);
  /** «Изменён» по событиям ядра сразу при получении, без ожидания кадра планировщика. */
  let modified = false;
  /** Правки после последней пачки журнала (у нового файла — после последней записи). */
  let dirty = false;
  let journalTimer = 0;
  let fullTimer = 0;
  let lastFull = performance.now();
  /** Последняя неудачная попытка полного файла (снимок или запись). */
  let failedAt = -Infinity;
  /** Срок следующего полного файла сам по себе (без просьбы пользователя): не реже
   * `FULL_MAX_MS` от прошлого и, после неудачи, не раньше чем через `fullIdleMs` от
   * неё. Причина отказа (место, другая вкладка, кодирование) за секунды не проходит,
   * а каждая попытка — снимок всего документа (≈0,45 с на 100 000 узлах): чаще
   * обычного шага полных файлов повторять незачем. Ctrl/Cmd+S и уход пробуют сразу. */
  const fullDue = () => Math.max(lastFull + FULL_MAX_MS, failedAt + fullIdleMs);
  let running: Promise<boolean> | null = null;
  let again: Promise<boolean> | null = null;
  /** Последняя показанная ошибка записи: повтор не показываем, пока запись не удастся. */
  let reported: string | null = null;
  let journalReported: string | null = null;
  let failure: string | null = null;
  let waiting: ((e: Snapshot) => void) | null = null;
  let failWaiting: (() => void) | null = null;
  /** Пачки дописываются по порядку прихода; запись файла ждёт последнюю. */
  let appended: Promise<boolean> = Promise.resolve(true);
  /** Пачки журнала по порядку прихода (номера с 1); `lost` — последняя пачка,
   * потерянная для журнала (не дописана в хранилище), `covered` — последняя,
   * вошедшая в записанный полный файл (пришла раньше его снимка). */
  let journalSeq = 0;
  let lost = 0;
  let covered = 0;
  let snapSeq = 0;
  const unjournaled = () => lost > covered;
  /** Последняя пачка не собрана ядром: правки ждут следующей. */
  let unbuilt = false;
  /** Полный файл в простое; пока журнал неполон — не позже срока `fullDue`. */
  const armFull = () => {
    clearTimeout(fullTimer);
    const left = unbuilt || unjournaled() ? fullDue() - performance.now() : fullIdleMs;
    fullTimer = window.setTimeout(() => void save(), Math.max(0, Math.min(fullIdleMs, left)));
  };
  /** Ждут ответа ядра на `flushJournal`. */
  const journalWaiters: ((done: Promise<boolean>) => void)[] = [];

  const edited = () => {
    dirty = true;
    clearTimeout(fullTimer);
    if (!stored) {
      fullTimer = window.setTimeout(() => void save(), AUTOSAVE_MS);
      return;
    }
    // Пачка — не позже `journalMs` после первой правки: правки подряд её не откладывают.
    if (!journalTimer) journalTimer = window.setTimeout(() => void flushJournal(), journalMs);
    armFull();
  };

  const off = ed.core.on((e) => {
    switch (e.type) {
      case 'modified':
        modified = e.modified;
        if (modified) edited();
        break;
      case 'document':
      case 'documentName':
      case 'pages':
        // Правка, пока документ уже изменён: журнал и полный файл — по таймерам.
        if (modified) edited();
        break;
      case 'journal':
        onJournal(e);
        break;
      case 'saveRequested':
        // После всей пачки событий: правка и Ctrl+S в одном кадре ядра присылают
        // `modified` позже `saveRequested`.
        queueMicrotask(() => void save());
        break;
      case 'snapshot':
        snapSeq = journalSeq;
        waiting?.(e);
        break;
      case 'error':
        // Снимок не удался или ядро остановлено: ответа не будет. Сообщение ядра уже показано.
        if (e.message.startsWith('Не удалось сохранить файл') || e.message.startsWith('Ядро остановлено')) {
          failure = e.message;
          failWaiting?.();
        }
        if (e.message.startsWith('Ядро остановлено')) for (const w of journalWaiters.splice(0)) w(Promise.resolve(false));
        break;
    }
  });

  const fail = (message: string) => {
    failure = message;
    if (reported === message) return;
    reported = message;
    ed.store.toast(message, 'error');
  };

  /** Пачка журнала от ядра (по `flushJournal` или перед снимком). У нового файла —
   * не нужна: её вберёт первая запись полного файла. */
  function onJournal(e: Journal): void {
    const seq = stored ? ++journalSeq : 0;
    /** Правки до этой пачки — только в памяти: заберёт их полный файл. */
    const lose = () => {
      lost = Math.max(lost, seq);
      armFull();
    };
    if (stored && e.failed) {
      // Ядро не собрало пачку (его ошибка уже показана) и начало не сдвинуло: правки
      // уйдут следующей пачкой, а не выйдет и она — полным файлом не позже `FULL_MAX_MS`.
      unbuilt = true;
      armFull();
      for (const w of journalWaiters.splice(0)) w(Promise.resolve(false));
      fullIfLong();
      return;
    }
    if (stored) unbuilt = false;
    if (stored && e.bytes) {
      const bytes = ownBuffer(e.bytes);
      const p = files.appendJournal(id, { session, batch: e.batch }, bytes).then(
        () => {
          journalReported = null;
          return true;
        },
        (err: unknown) => {
          lose();
          const message = `Не удалось записать журнал изменений: ${err instanceof Error ? err.message : String(err)}`;
          if (journalReported !== message) {
            journalReported = message;
            ed.store.toast(message, 'error');
          }
          return false;
        },
      );
      appended = appended.then(() => p);
    }
    // Всё до этой пачки — в ней (правки после неё снова поставят признак).
    if (stored) dirty = false;
    const done = appended;
    for (const w of journalWaiters.splice(0)) w(done);
    fullIfLong();
  }

  /** Долгая работа без пауз: полный файл не реже `FULL_MAX_MS` (после неудачи — по
   * `fullDue`). Пачка перед снимком идущей записи второй снимок не просит. */
  function fullIfLong(): void {
    if (stored && modified && !running && performance.now() > fullDue()) void save();
  }

  function flushJournal(): Promise<boolean> {
    clearTimeout(journalTimer);
    journalTimer = 0;
    if (!stored || ed.store.state.stopped) return Promise.resolve(!dirty && !unjournaled());
    return new Promise<boolean>((resolve) => {
      journalWaiters.push((done) => void done.then((ok) => resolve(ok && !dirty && !unjournaled())));
      ed.send({ type: 'flushJournal' });
    });
  }

  async function write(): Promise<boolean> {
    clearTimeout(fullTimer);
    if (!modified) return true;
    if (ed.store.state.stopped) {
      // Плашка остановки уже на экране: здесь — только причина для вопроса при уходе.
      failure = 'Ядро остановлено — изменения не сохранены';
      return false;
    }
    const snap = await new Promise<Snapshot | null>((resolve) => {
      waiting = resolve;
      failWaiting = () => resolve(null);
      ed.send({ type: 'snapshot', thumbnail: true });
    });
    waiting = null;
    failWaiting = null;
    if (!snap) {
      failedAt = performance.now();
      return false;
    }
    const seen = snapSeq;
    const wasStored = stored;
    // У нового файла журнала нет: правки до снимка вберёт запись (отказ вернёт признак).
    if (!wasStored) dirty = false;
    try {
      // Пачка с правками до снимка дописывается раньше файла.
      await appended;
      const thumb = snap.thumbnail ? ownBuffer(snap.thumbnail) : snap.keepThumbnail ? 'keep' : null;
      const journal = wasStored ? { session, upTo: snap.journal } : null;
      const meta = await files.write(id, ownBuffer(snap.bytes), thumb, base, journal);
      base = meta.version;
      stored = true;
      lastFull = performance.now();
      covered = Math.max(covered, seen);
      ed.send({ type: 'markSaved', revision: snap.revision });
      reported = null;
      failure = null;
      return true;
    } catch (err) {
      if (!wasStored) dirty = true;
      failedAt = performance.now();
      fail(`Не удалось сохранить файл: ${err instanceof Error ? err.message : String(err)}`);
      return false;
    }
  }

  /** Одна запись за раз; просьбы во время записи сливаются в одну следующую. */
  function save(): Promise<boolean> {
    if (!running) {
      const p = write().finally(() => {
        if (running === p) running = null;
      });
      running = p;
      return p;
    }
    again ??= running.then(() => {
      again = null;
      return save();
    });
    return again;
  }

  const onHide = () => {
    void flushJournal();
    void save();
  };
  const onVisibility = () => {
    if (document.visibilityState === 'hidden') void flushJournal();
  };
  window.addEventListener('pagehide', onHide);
  document.addEventListener('visibilitychange', onVisibility);

  return {
    save,
    flushJournal,
    pending: () => dirty || unjournaled() || (!stored && modified),
    settled: () =>
      new Promise<boolean>((resolve) => {
        const timer = window.setTimeout(() => resolve(false), SETTLE_MS);
        void appended.then(() => {
          clearTimeout(timer);
          resolve(true);
        });
      }),
    failure: () => failure,
    dispose() {
      clearTimeout(journalTimer);
      clearTimeout(fullTimer);
      off();
      window.removeEventListener('pagehide', onHide);
      document.removeEventListener('visibilitychange', onVisibility);
      failWaiting?.();
      for (const w of journalWaiters.splice(0)) w(Promise.resolve(false));
    },
  };
}
