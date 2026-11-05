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
//   снимка (в браузере — той же транзакцией, на сервере — после ответа сервера: сбой
//   между ними оставит пачки, и открытие применит их поверх файла без вреда).
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
// Файл с журналом сервера (`opts.sync`): правки идут по соединению файла, а полный файл пишется только первый раз, чтобы журналу
// было к чему лечь. Журнал браузера держит неподтверждённое и снимается по подтверждению сервера (`journalCovered`). «Сохранить» —
// дождаться сервера; не дождались (нет сети) — уход спрашивает, правки при этом в журнале браузера и уйдут при следующем открытии.
// Запись через сеть (`files.remote`): после сбоя она повторяется сама с растущей паузой, от `FULL_IDLE_MS` до
// десяти минут (удача или правка начинают заново), кроме отказов, которые повтор не исправит. Без сети и за
// ночь со скрытой вкладкой выходит несколько десятков снимков, а не сотни. Закрытие вкладки спрашивает, пока
// правки есть только в памяти или пачка ещё дописывается: незавершённая запись в IndexedDB откатится.
// Новый файл без записи, а файл уже записал другой человек: журнала нет и сливать нечего, документы независимы.
// Запись больше не пробуется, правки спасает человек: новым файлом или копией на диск (`onDiverged`).

import { NEW_FILE_TAKEN, type FileMeta } from '../core/file-list';
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

/** Коды отказа, которые повтор записи не исправит: нужен человек. */
const FINAL_FAILURES: ReadonlySet<unknown> = new Set([
  'CONFLICT',
  'FORBIDDEN',
  'NOT_FOUND',
  'UNAUTHORIZED',
  'TOO_LARGE',
  'BAD_CONTAINER',
  'BAD_REQUEST',
  'HASH_MISMATCH',
  'TOO_MANY_IMAGES',
]);
/** Пауза перед автоповтором записи — во столько раз больше `fullIdleMs`: 30 с, 1, 2, 5 и 10 минут. */
const RETRY_STEPS = [1, 2, 4, 10, 20];

/** `p` или, если он не успел за `ms`, — `fallback`; таймер снимается в любом случае. */
export function withTimeout<T>(p: Promise<T>, ms: number, fallback: T): Promise<T> {
  return new Promise<T>((resolve) => {
    const timer = window.setTimeout(() => resolve(fallback), ms);
    void p.then((v) => {
      clearTimeout(timer);
      resolve(v);
    });
  });
}

export interface SaverOptions {
  /** Тесты: интервалы журнала и полного файла. */
  journalMs?: number;
  fullIdleMs?: number;
  /** Наибольший сеанс среди пачек журнала файла при открытии (`null` — пачек нет):
   * сеанс этого редактора — после него, даже если часы ушли назад. */
  journalSession?: number | null;
  /** Файл без записи уже записал другой человек: правки больше не лягут в него. */
  onDiverged?: () => void;
  /** Файл на сервере с журналом: правки идут соединением файла, а не записью целиком. */
  sync?: boolean;
  /** Читатель: первую запись делает не он. */
  viewer?: boolean;
  /** Файл теперь есть на сервере: можно подключаться. */
  onStored?: () => void;
  /** Первую запись опередил другой, своих правок ещё не было: открыть файл заново, его содержимое и есть правда. */
  onTaken?: () => void;
}

/** Сколько «Сохранить» и уход ждут подтверждения сервера. */
export const SYNC_WAIT_MS = 5000;
/** Превью при правках журналом — не чаще этого (раздел 6.2). */
export const THUMB_MS = 60_000;
/** Текст вопроса при уходе: правки не потеряются, но и на сервере их нет. */
export const NOT_ON_SERVER =
  'Правки ещё не дошли до сервера. Они сохранены в этом браузере и уйдут, когда вы откроете файл снова при подключении';

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
  /** Поколение файла, на котором основана следующая запись. */
  version(): number;
  /** Есть ли у файла запись на сервере. */
  stored(): boolean;
  /** Человек согласился уйти: закрытие вкладки больше не спрашивает. */
  release(): void;
  /** Правки — в новый файл (когда в этот они больше не лягут). `null` — не вышло, причина показана. */
  saveAsNew(name: string): Promise<FileMeta | null>;
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
export function createSaver(ed: Editor, files: FileStore, id: string, base: number, stored: boolean, opts: SaverOptions = {}): Saver {
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
  /** Пауза после неудачи: растёт с каждым автоповтором через сеть. */
  let pause = fullIdleMs;
  let retries = 0;
  const resetRetries = () => {
    retries = 0;
    pause = fullIdleMs;
  };
  /** Срок следующего полного файла сам по себе (без просьбы пользователя): не реже
   * `FULL_MAX_MS` от прошлого и, после неудачи, не раньше чем через `pause` от
   * неё. Причина отказа (место, другая вкладка, кодирование) за секунды не проходит,
   * а каждая попытка — снимок всего документа (≈0,45 с на 100 000 узлах): чаще
   * обычного шага полных файлов повторять незачем. Ctrl/Cmd+S и уход пробуют сразу. */
  const fullDue = () => Math.max(lastFull + FULL_MAX_MS, failedAt + pause);
  let running: Promise<boolean> | null = null;
  let again: Promise<boolean> | null = null;
  /** Последняя показанная ошибка записи: повтор не показываем, пока запись не удастся. */
  let reported: string | null = null;
  let journalReported: string | null = null;
  /** Последнее показанное предупреждение удачной записи (превью не записано). */
  let warned: string | null = null;
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
  /** Пачки, отправленные в хранилище и ещё не подтверждённые. */
  let appending = 0;
  /** Файл без записи уже записал другой человек: запись бессмысленна. */
  let diverged = false;
  /** Правки уже в новом файле: этому редактору записывать нечего. */
  let rescued = false;
  let disposed = false;
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

  /** Подтверждение сервера: ждущие «Сохранить» и уход. */
  let unacked = false;
  let ackWaiters: ((ok: boolean) => void)[] = [];
  let thumbTimer = 0;
  /** Первая запись нужна и без правок: журналу надо к чему лечь. */
  let needBase = opts.sync === true && !stored && opts.viewer !== true;
  const synced = () => opts.sync === true && stored;

  const edited = () => {
    dirty = true;
    resetRetries();
    clearTimeout(fullTimer);
    if (!stored) {
      fullTimer = window.setTimeout(() => void save(), AUTOSAVE_MS);
      return;
    }
    if (synced()) {
      if (!journalTimer) journalTimer = window.setTimeout(() => void flushJournal(), journalMs);
      if (!thumbTimer && files.putThumb) {
        thumbTimer = window.setTimeout(() => {
          thumbTimer = 0;
          ed.send({ type: 'thumbnail' });
        }, opts.fullIdleMs ?? THUMB_MS);
      }
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
      case 'journalCovered':
        void files.cutJournal?.(id, { session, batch: e.batch }).catch(() => {
          // Не снялось — останется до следующего раза, при открытии его пропустит Loro.
        });
        break;
      case 'syncState':
        unacked = e.unacked;
        if (!unacked) {
          for (const w of ackWaiters.splice(0)) w(true);
          failure = null;
        }
        break;
      case 'thumbnail':
        if (!e.keep) files.putThumb?.(id, e.bytes ? ownBuffer(e.bytes) : null);
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
      appending++;
      const p = files.appendJournal(id, { session, batch: e.batch }, bytes).then(
        () => {
          appending--;
          journalReported = null;
          return true;
        },
        (err: unknown) => {
          appending--;
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

  async function takeSnapshot(): Promise<Snapshot | null> {
    const snap = await new Promise<Snapshot | null>((resolve) => {
      waiting = resolve;
      failWaiting = () => resolve(null);
      ed.send(files.fullHistory ? { type: 'snapshot', thumbnail: true, full: true } : { type: 'snapshot', thumbnail: true });
    });
    waiting = null;
    failWaiting = null;
    return snap;
  }

  /** Файл ведёт журнал сервера: «записано» — это подтвердил сервер. */
  async function waitServer(): Promise<boolean> {
    // Человек сохраняет или уходит: превью не ждёт своего срока.
    if (thumbTimer) {
      clearTimeout(thumbTimer);
      thumbTimer = 0;
      ed.send({ type: 'thumbnail' });
    }
    const flushed = await flushJournal();
    if (!unacked) return flushed;
    const ok = await new Promise<boolean>((resolve) => {
      const timer = window.setTimeout(() => {
        ackWaiters = ackWaiters.filter((w) => w !== done);
        resolve(false);
      }, SYNC_WAIT_MS);
      const done = (v: boolean) => {
        clearTimeout(timer);
        resolve(v);
      };
      ackWaiters.push(done);
    });
    if (!ok) failure = NOT_ON_SERVER;
    return ok;
  }

  async function write(): Promise<boolean> {
    clearTimeout(fullTimer);
    if (synced()) return rescued ? true : waitServer();
    if ((!modified && !needBase) || rescued) return true;
    // Отказ не изменится, а причина уже показана: вопрос при уходе назовёт её.
    if (diverged) return false;
    if (ed.store.state.stopped) {
      // Плашка остановки уже на экране: здесь — только причина для вопроса при уходе.
      failure = 'Ядро остановлено — изменения не сохранены';
      return false;
    }
    const snap = await takeSnapshot();
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
      needBase = false;
      if (opts.sync) opts.onStored?.();
      lastFull = performance.now();
      covered = Math.max(covered, seen);
      resetRetries();
      ed.send({ type: 'markSaved', revision: snap.revision });
      reported = null;
      failure = null;
      // Файл записан, превью — нет: одно уведомление, пока запись не пройдёт без него.
      if (meta.warning && warned !== meta.warning) ed.store.toast(meta.warning, 'error');
      warned = meta.warning ?? null;
      return true;
    } catch (err) {
      if (!wasStored) dirty = true;
      failedAt = performance.now();
      const code = (err as { code?: unknown }).code;
      if (files.remote && !wasStored && code === 'CONFLICT' && !modified && opts.onTaken) {
        opts.onTaken();
        return false;
      }
      if (files.remote && !wasStored && code === 'CONFLICT') {
        diverged = true;
        fail(NEW_FILE_TAKEN);
        opts.onDiverged?.();
        return false;
      }
      fail(`Не удалось сохранить файл: ${err instanceof Error ? err.message : String(err)}`);
      if (files.remote && !FINAL_FAILURES.has(code) && !disposed) {
        pause = fullIdleMs * RETRY_STEPS[Math.min(retries++, RETRY_STEPS.length - 1)];
        clearTimeout(fullTimer);
        fullTimer = window.setTimeout(() => void save(), pause);
      }
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
  const pending = () => !rescued && (dirty || unjournaled() || (!stored && modified));
  const onBeforeUnload = (e: BeforeUnloadEvent) => {
    if (!pending() && appending === 0) return;
    e.preventDefault();
    e.returnValue = '';
  };
  window.addEventListener('pagehide', onHide);
  document.addEventListener('visibilitychange', onVisibility);
  if (files.remote) window.addEventListener('beforeunload', onBeforeUnload);
  const release = () => window.removeEventListener('beforeunload', onBeforeUnload);

  if (needBase) queueMicrotask(() => void save());

  return {
    save,
    flushJournal,
    pending,
    stored: () => stored,
    settled: () =>
      withTimeout(
        appended.then(() => true),
        SETTLE_MS,
        false,
      ),
    failure: () => failure,
    version: () => base,
    release,
    async saveAsNew(name) {
      const create = files.createFrom?.bind(files);
      if (!create) return null;
      const snap = await takeSnapshot();
      try {
        if (!snap) return null;
        const meta = await create(name, ownBuffer(snap.bytes));
        rescued = true;
        return meta;
      } catch (err) {
        ed.store.toast(`Не удалось сохранить новым файлом: ${err instanceof Error ? err.message : String(err)}`, 'error');
        return null;
      }
    },
    dispose() {
      disposed = true;
      release();
      clearTimeout(journalTimer);
      clearTimeout(fullTimer);
      clearTimeout(thumbTimer);
      for (const w of ackWaiters.splice(0)) w(false);
      off();
      window.removeEventListener('pagehide', onHide);
      document.removeEventListener('visibilitychange', onVisibility);
      failWaiting?.();
      for (const w of journalWaiters.splice(0)) w(Promise.resolve(false));
    },
  };
}
