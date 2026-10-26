// Настройки пользователя в IndexedDB (этап 6, спека 7.6): по ключу на поле в хранилище
// `settings` worker'а ввода-вывода (в Tauri — файл конфигурации, `tauri.ts`). Вкладка
// пишет только изменённые поля, поэтому две вкладки не затирают настройки друг друга
// (так было с одной записью в `localStorage`); чужие изменения приходят событием и
// применяются на лету. При первом запуске настройки переносятся из прежней записи
// `localStorage` (`ferrum.settings.v1`), и она удаляется. Тему до первого рисования
// читает `theme-init.js` — для него тема зеркалится в `localStorage` (`ferrum.theme`).
// Ошибка чтения или записи — одно уведомление, пока запись снова не удастся.

import { loadSettings, parseSettings, parseSettingsPatch, diffSettings, SETTINGS_KEY, THEME_KEY, type Settings } from './settings';

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

export async function openSettings(backend: SettingsBackend, legacy: Storage | null): Promise<SettingsStore> {
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
  try {
    raw = await backend.settings();
  } catch (e) {
    report(`Не удалось прочитать настройки: ${text(e)}`);
  }
  let current: Settings;
  if (Object.keys(raw).length === 0 && hasLegacy(legacy)) {
    // Первый запуск после этапа 6: перенести прежнюю запись и удалить её.
    current = loadSettings(legacy);
    try {
      await backend.setSettings({ ...current });
      legacy?.removeItem(SETTINGS_KEY);
    } catch (e) {
      report(`Не удалось сохранить настройки: ${text(e)}`);
    }
  } else current = parseSettings(raw);
  mirrorTheme(legacy, current.theme);

  const listeners = new Set<(p: Partial<Settings>) => void>();
  const off = backend.onSettings((p) => {
    // Запись другой вкладки — не доверенная: только верные поля и только изменения.
    const changed = diffSettings(current, parseSettingsPatch(p));
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
      backend.setSettings(changed).then(
        () => (reported = null),
        (e: unknown) => report(`Не удалось сохранить настройки: ${text(e)}`),
      );
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
