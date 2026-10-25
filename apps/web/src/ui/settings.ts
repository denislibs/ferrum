
// Настройки пользователя: тема, привязка к пикселям, отличия раскладки клавиш,
// ширина панелей. Хранятся в localStorage (полноценное хранилище — этап 6);
// доступ к нему может бросить (закрытые cookie, приватный режим) или вернуть
// мусор — тогда настройки по умолчанию, а запись тихо не удаётся.

export type ThemeChoice = 'dark' | 'light' | 'system';

export interface Settings {
  theme: ThemeChoice;
  pixelSnap: boolean;
  /** JSON отличий раскладки (`ferrum-keymap`), как его отдаёт ядро; пусто — умолчания. */
  keymap: string;
  leftWidth: number;
  rightWidth: number;
  pagesOpen: boolean;
}

export const SETTINGS_KEY = 'ferrum.settings.v1';
/** Предел размера записи: раскладка ядра — до 64 КБ, остальное — мелочь. */
const MAX_BYTES = 80 * 1024;

export const DEFAULT_SETTINGS: Settings = {
  theme: 'dark',
  pixelSnap: true,
  keymap: '',
  leftWidth: 248,
  rightWidth: 248,
  pagesOpen: true,
};

/** Хранилище: `localStorage`, если доступ к нему не бросает. */
export function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

const isWidth = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v) && v >= 120 && v <= 800;

/** Читает настройки; неверные поля заменяются умолчаниями поодиночке. */
export function loadSettings(store: Pick<Storage, 'getItem'> | null): Settings {
  let raw: unknown = null;
  try {
    const text = store?.getItem(SETTINGS_KEY);
    if (text && text.length <= MAX_BYTES) raw = JSON.parse(text);
  } catch {
    raw = null;
  }
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    theme: o.theme === 'light' || o.theme === 'system' || o.theme === 'dark' ? o.theme : DEFAULT_SETTINGS.theme,
    pixelSnap: typeof o.pixelSnap === 'boolean' ? o.pixelSnap : DEFAULT_SETTINGS.pixelSnap,
    keymap: typeof o.keymap === 'string' && o.keymap.length <= 64 * 1024 ? o.keymap : DEFAULT_SETTINGS.keymap,
    leftWidth: isWidth(o.leftWidth) ? o.leftWidth : DEFAULT_SETTINGS.leftWidth,
    rightWidth: isWidth(o.rightWidth) ? o.rightWidth : DEFAULT_SETTINGS.rightWidth,
    pagesOpen: typeof o.pagesOpen === 'boolean' ? o.pagesOpen : DEFAULT_SETTINGS.pagesOpen,
  };
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
