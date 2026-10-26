// Настройки пользователя в IndexedDB (этап 6, спека 7.6): по ключу на поле в хранилище
// `settings` worker'а ввода-вывода (в Tauri — файл конфигурации, `tauri.ts`). Вкладка
// пишет только изменённые поля, поэтому две вкладки не затирают настройки друг друга
// (так было с одной записью в `localStorage`); чужие изменения приходят событием и
// применяются на лету. При первом запуске настройки переносятся из прежней записи
// `localStorage` (`ferrum.settings.v1`), и она удаляется. Тему до первого рисования
// читает `theme-init.js` — для него тема зеркалится в `localStorage` (`ferrum.theme`).
// Ошибка чтения или записи — одно уведомление, пока запись снова не удастся. Чтение
// при запуске — не дольше `SETTLE_MS` (база не отвечает — умолчания и уведомление,
// экран не ждёт). Раскладка панелей (ширины, страницы) у каждой вкладки своя: чужие
// значения только запоминаются базой, в `current` этой вкладки не входят.
// Запись идёт через worker ввода-вывода, а его уничтожают вместе со страницей раньше,
// чем запись дойдёт до базы (перезагрузка, закрытие сразу после смены). Поэтому каждое
// изменение сначала синхронно ложится в запасную запись `localStorage`
// (`ferrum.settings.pending`, ≤ 80 КБ) и убирается из неё после удачной записи; при
// запуске оставшиеся поля применяются поверх базы и записываются в неё (база не
// прочитана — только показываются).

import { SETTLE_MS } from './saver';
import { MAX_SETTINGS_BYTES } from '../core/limits';
import { loadSettings, parseSettings, parseSettingsPatch, diffSettings, PENDING_KEY, SETTINGS_KEY, THEME_KEY, type Settings } from './settings';

/** Поля раскладки панелей: у каждой вкладки свои, изменения других вкладок не применяются. */
const LOCAL_FIELDS: readonly (keyof Settings)[] = ['leftWidth', 'rightWidth', 'pagesOpen'];

/** Где лежат поля: worker ввода-вывода (`IdbFileStore`) или Tauri. */
export interface SettingsBackend {
  /** Все поля (не доверенные). */
  settings(): Promise<Record<string, unknown>>;
  /** Записать только эти поля. */
  setSettings(patch: Partial<Settings>): Promise<void>;
  /** Поля, записанные другой вкладкой или окном; возвращает отписку. */
  onSettings(listener: (patch: Record<string, unknown>) => void): () => void;
}

export interface SettingsStore {
  /** Нынешние настройки: прочитанные при запуске, свои и чужие изменения. */
  readonly current: Settings;
  /** Записать изменённые поля (те же значения — без записи). */
  update(patch: Partial<Settings>): void;
  /** Вернуть поля к прежним без записи (чужое значение не принято, например раскладка
   * клавиш, которую отверг ядро этой вкладки). */
  rollback(patch: Partial<Settings>): void;
  /** Поля, изменённые в другой вкладке; возвращает отписку. */
  onChange(listener: (patch: Partial<Settings>) => void): () => void;
  /** Ошибки чтения и записи (каждая — один раз); ошибка до подписки отдаётся первому. */
  onError(listener: (message: string) => void): () => void;
  dispose(): void;
}

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

function hasLegacy(store: Storage | null): boolean {
  try {
    return !!store?.getItem(SETTINGS_KEY);
  } catch {
    return false;
  }
}

function mirrorTheme(store: Storage | null, theme: Settings['theme']): void {
  try {
    store?.setItem(THEME_KEY, theme);
  } catch {
    // Закрытое хранилище: тема до рисования — тёмная, после запуска — верная.
  }
}

/** Запасная запись несохранённых полей (не доверенная: только верные поля). */
function readPending(store: Storage | null): Partial<Settings> {
  try {
    const text = store?.getItem(PENDING_KEY);
    if (!text || text.length > MAX_SETTINGS_BYTES) return {};
    const raw: unknown = JSON.parse(text);
    return typeof raw === 'object' && raw !== null && !Array.isArray(raw) ? parseSettingsPatch(raw as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** Запасная запись: `put` — дописать поля, `drop` — убрать. Общая для вкладок, поэтому
 * читается заново перед каждой правкой. Новые значения не влезли в предел — этих полей
 * в ней нет вовсе: их прежние значения при запуске перекрыли бы новые из базы. */
function editPending(store: Storage | null, put: Partial<Settings>, drop: readonly string[] = []): void {
  try {
    if (!store) return;
    const next: Record<string, unknown> = { ...readPending(store), ...put };
    for (const k of drop) delete next[k];
    let text = JSON.stringify(next);
    if (text.length > MAX_SETTINGS_BYTES) {
      for (const k of Object.keys(put)) delete next[k];
      text = JSON.stringify(next);
    }
    if (Object.keys(next).length === 0 || text.length > MAX_SETTINGS_BYTES) store.removeItem(PENDING_KEY);
    else store.setItem(PENDING_KEY, text);
  } catch {
    // Нет места: хотя бы убрать прежние значения этих полей (запись меньше); запись в
    // базу всё равно идёт. Хранилище закрыто — запасной и не прочитать.
    try {
      const rest: Record<string, unknown> = readPending(store);
      for (const k of [...Object.keys(put), ...drop]) delete rest[k];
      if (Object.keys(rest).length === 0) store?.removeItem(PENDING_KEY);
      else store?.setItem(PENDING_KEY, JSON.stringify(rest));
    } catch {
      // Ничего не сделать.
    }
  }
}

/** `readMs` — предел чтения при запуске (тесты — короче). */
export async function openSettings(backend: SettingsBackend, legacy: Storage | null, readMs = SETTLE_MS): Promise<SettingsStore> {
  const errors = new Set<(m: string) => void>();
  let pendingError: string | null = null;
  let reported: string | null = null;
  const report = (m: string) => {
    if (reported === m) return;
    reported = m;
    if (errors.size === 0) pendingError = m;
    for (const l of errors) l(m);
  };

  let raw: Record<string, unknown> = {};
  let readFailed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    raw = await Promise.race([
      backend.settings(),
      new Promise<never>((_, reject) => (timer = setTimeout(() => reject(new Error('хранилище не отвечает')), readMs))),
    ]);
  } catch (e) {
    readFailed = true;
    report(`Не удалось прочитать настройки: ${text(e)}`);
  } finally {
    clearTimeout(timer);
  }
  /** Поля прошлого запуска, не дошедшие до базы: поверх прочитанного. */
  const unsaved = readPending(legacy);
  /** Номер последней записи каждого поля: удачная запись убирает поле из запасной,
   * только если с тех пор его не меняли. */
  const gen: Partial<Record<keyof Settings, number>> = {};
  let writes = 0;
  const save = (changed: Partial<Settings>) => {
    const mine = ++writes;
    const keys = Object.keys(changed) as (keyof Settings)[];
    for (const k of keys) gen[k] = mine;
    // Синхронно, до запроса worker'у: страницу могут закрыть раньше его ответа.
    editPending(legacy, changed);
    backend.setSettings(changed).then(
      () => {
        reported = null;
        editPending(
          legacy,
          {},
          keys.filter((k) => gen[k] === mine),
        );
      },
      (e: unknown) => report(`Не удалось сохранить настройки: ${text(e)}`),
    );
  };
  let current: Settings;
  if (readFailed) {
    // База не прочитана — переносить нельзя (затёрли бы её поля прежней записью):
    // прежняя запись и запасная, если есть, — только для показа, без записи и удаления.
    current = { ...(hasLegacy(legacy) ? loadSettings(legacy) : parseSettings({})), ...unsaved };
  } else if (Object.keys(raw).length === 0 && hasLegacy(legacy)) {
    // Первый запуск после этапа 6: перенести прежнюю запись и удалить её.
    current = { ...loadSettings(legacy), ...unsaved };
    try {
      await backend.setSettings({ ...current });
      legacy?.removeItem(SETTINGS_KEY);
      editPending(legacy, {}, Object.keys(unsaved));
    } catch (e) {
      report(`Не удалось сохранить настройки: ${text(e)}`);
    }
  } else {
    current = { ...parseSettings(raw), ...unsaved };
    if (Object.keys(unsaved).length > 0) save(unsaved);
  }
  mirrorTheme(legacy, current.theme);

  const listeners = new Set<(p: Partial<Settings>) => void>();
  const off = backend.onSettings((p) => {
    // Запись другой вкладки — не доверенная: только верные поля и только изменения.
    const changed = diffSettings(current, parseSettingsPatch(p));
    for (const k of LOCAL_FIELDS) delete changed[k];
    if (Object.keys(changed).length === 0) return;
    current = { ...current, ...changed };
    if (changed.theme) mirrorTheme(legacy, changed.theme);
    for (const l of listeners) l(changed);
  });

  return {
    get current() {
      return current;
    },
    update(patch) {
      const changed = diffSettings(current, patch);
      if (Object.keys(changed).length === 0) return;
      current = { ...current, ...changed };
      if (changed.theme) mirrorTheme(legacy, changed.theme);
      save(changed);
    },
    rollback(patch) {
      const changed = diffSettings(current, parseSettingsPatch(patch));
      current = { ...current, ...changed };
      if (changed.theme) mirrorTheme(legacy, changed.theme);
    },
    onChange(l) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    onError(l) {
      errors.add(l);
      if (pendingError) {
        l(pendingError);
        pendingError = null;
      }
      return () => errors.delete(l);
    },
    dispose() {
      off();
      listeners.clear();
      errors.clear();
    },
  };
}
