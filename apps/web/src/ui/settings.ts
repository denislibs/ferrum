
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей, вид экрана файлов (сетка или список). С этапа 6 хранятся в
// IndexedDB по полю (`settings-store.ts`); здесь — поля, их разбор (значения из
// хранилища не доверенные: неверное поле — умолчание) и прежняя запись
// `localStorage`, из которой настройки переносятся при первом запуске.

export type ThemeChoice = 'dark' | 'light' | 'system';

export interface Settings {
  theme: ThemeChoice;
  pixelSnap: boolean;
  /** JSON отличий раскладки (`ferrum-keymap`), как его отдаёт ядро; пусто — умолчания. */
  keymap: string;
  leftWidth: number;
  rightWidth: number;
  pagesOpen: boolean;
  /** Экран файлов: карточки сеткой или строки списком (поле появилось в этапе 6;
   * в старых записях его нет — тогда сетка). */
  filesView: FilesView;
}

export type FilesView = 'grid' | 'list';

import { MAX_KEYMAP_BYTES, MAX_SETTINGS_BYTES } from '../core/limits';

/** Прежняя запись настроек в `localStorage` (до этапа 6): переносится и удаляется. */
export const SETTINGS_KEY = 'ferrum.settings.v1';
/** Зеркало темы в `localStorage` для `theme-init.js` (тема до первого рисования). */
export const THEME_KEY = 'ferrum.theme';

/** Пределы ширины боковых панелей. */
export const PANEL_MIN = 200;
export const PANEL_MAX = 480;

export function clampPanel(w: number): number {
  return Math.round(Math.min(PANEL_MAX, Math.max(PANEL_MIN, w)));
}

export const DEFAULT_SETTINGS: Settings = {
  theme: 'dark',
  pixelSnap: true,
  keymap: '',
  leftWidth: 248,
  rightWidth: 248,
  pagesOpen: true,
  filesView: 'grid',
};

/** Хранилище: `localStorage`, если доступ к нему не бросает. */
export function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

const utf8 = new TextEncoder();
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** Проверка каждого поля: верное значение или `undefined`. */
const FIELDS: { [K in keyof Settings]: (v: unknown) => Settings[K] | undefined } = {
  theme: (v) => (v === 'light' || v === 'system' || v === 'dark' ? v : undefined),
  pixelSnap: (v) => (typeof v === 'boolean' ? v : undefined),
  // Предел — в байтах UTF-8, как у записи настроек в worker'е (80 КБ на запись).
  keymap: (v) => (typeof v === 'string' && v.length <= MAX_KEYMAP_BYTES && utf8.encode(v).length <= MAX_KEYMAP_BYTES ? v : undefined),
  // Ширина из хранилища — в пределах края панели (как при перетаскивании).
  leftWidth: (v) => (isNumber(v) ? clampPanel(v) : undefined),
  rightWidth: (v) => (isNumber(v) ? clampPanel(v) : undefined),
  pagesOpen: (v) => (typeof v === 'boolean' ? v : undefined),
  filesView: (v) => (v === 'list' || v === 'grid' ? v : undefined),
};

/** Только верные известные поля (чужое изменение из другой вкладки, запись базы). */
export function parseSettingsPatch(o: Record<string, unknown>): Partial<Settings> {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(FIELDS) as (keyof Settings)[]) {
    if (!(k in o)) continue;
    const v = FIELDS[k](o[k]);
    if (v !== undefined) out[k] = v;
  }
  return out as Partial<Settings>;
}

/** Все поля: неверные и отсутствующие — умолчания поодиночке. */
export function parseSettings(o: Record<string, unknown>): Settings {
  return { ...DEFAULT_SETTINGS, ...parseSettingsPatch(o) };
}

/** Поля `patch`, отличные от `base` (`undefined` — не трогать). */
export function diffSettings(base: Settings, patch: Partial<Settings>): Partial<Settings> {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(patch) as (keyof Settings)[]) {
    if (patch[k] !== undefined && patch[k] !== base[k]) out[k] = patch[k];
  }
  return out as Partial<Settings>;
}

/** Читает прежнюю запись `localStorage`; неверные поля заменяются умолчаниями поодиночке. */
export function loadSettings(store: Pick<Storage, 'getItem'> | null): Settings {
  let raw: unknown = null;
  try {
    const text = store?.getItem(SETTINGS_KEY);
    if (text && text.length <= MAX_SETTINGS_BYTES) raw = JSON.parse(text);
  } catch {
    raw = null;
  }
  return parseSettings((typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>);
}

/** Записывает часть настроек поверх сохранённых: экран файлов и редактор пишут
 * каждый свои поля и не затирают чужие. */
export function updateSettings(store: Pick<Storage, 'getItem' | 'setItem'> | null, patch: Partial<Settings>): boolean {
  return saveSettings(store, { ...loadSettings(store), ...patch });
}

/** Записывает настройки; `false` — не удалось (нет места, запрещено). */
export function saveSettings(store: Pick<Storage, 'setItem'> | null, s: Settings): boolean {
  try {
    if (!store) return false;
    store.setItem(SETTINGS_KEY, JSON.stringify(s));
    return true;
  } catch {
    return false;
  }
}

/** Тема интерфейса для выбора «системная» — по `prefers-color-scheme`. */
export function resolveTheme(choice: ThemeChoice, prefersDark: boolean): 'dark' | 'light' {
  if (choice === 'system') return prefersDark ? 'dark' : 'light';
  return choice;
}

/** Холст темы (токен `canvas`): цвет, которым заполнены страница без своего фона и
 * панель свойств. Единственная запись на стороне интерфейса; `theme.css` и
 * `crates/ferrum-wasm/src/core.rs` (`CANVAS_DARK`, `CANVAS_LIGHT`) повторяют её
 * по необходимости, а `unit/settings.test.ts` сверяет все три. */
export const THEME_CANVAS = { dark: '#1a1918ff', light: '#e9e7e4ff' } as const;
