// Сочетания клавиш в интерфейсе: разбор и каноническая запись (как в Rust:
// «Ctrl+Alt+Shift+Meta+KeyZ»), подписи для подсказок и настроек, сочетание из
// события клавиатуры (по физической клавише `code` — работает на русской
// раскладке), сочетания буфера обмена. Подписи — для текущей раскладки через
// `navigator.keyboard.getLayoutMap()`, где доступно; без неё — US, как в Figma.

import type { Platform } from '../core/core';

export interface Shortcut {
  code: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
}

/** Раскладка: физическая клавиша → символ (`KeyboardLayoutMap`). */
export type LayoutMap = ReadonlyMap<string, string>;

const MODS = ['Ctrl', 'Alt', 'Shift', 'Meta'] as const;

/** Клавиши-модификаторы сами по себе сочетанием не бывают. */
const MODIFIER_CODES = new Set([
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
  'OSLeft',
  'OSRight',
  'CapsLock',
  'Fn',
]);

export function parseShortcut(s: string): Shortcut | null {
  const parts = s.split('+');
  const code = parts.pop();
  if (!code) return null;
  const sc: Shortcut = { code, ctrl: false, alt: false, shift: false, meta: false };
  for (const p of parts) {
    const key = p === 'Cmd' ? 'meta' : (p.toLowerCase() as keyof Shortcut);
    if (!(MODS as readonly string[]).includes(p) && p !== 'Cmd') return null;
    if (sc[key]) return null;
    (sc as unknown as Record<string, boolean>)[key] = true;
  }
  return sc;
}

/** Каноническая запись: модификаторы в порядке Ctrl, Alt, Shift, Meta. */
export function formatShortcut(sc: Shortcut): string {
  const mods = [sc.ctrl && 'Ctrl', sc.alt && 'Alt', sc.shift && 'Shift', sc.meta && 'Meta'].filter(Boolean);
  return [...mods, sc.code].join('+');
}

const NAMED: Record<string, string> = {
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Minus: '-',
  Equal: '=',
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
  Escape: 'Esc',
};

/** Подпись клавиши: буква, цифра или знак — по раскладке, остальное — по таблице. */
export function keyLabel(code: string, layout?: LayoutMap): string {
  const printable = /^(Key[A-Z]|Digit\d|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Backquote)$/;
  const fromLayout = printable.test(code) ? layout?.get(code) : undefined;
  if (fromLayout && fromLayout.trim()) return fromLayout.toUpperCase();
  const m = /^(?:Key|Digit)(.)$/.exec(code);
  if (m) return m[1];
  return NAMED[code] ?? code;
}

/** Подпись сочетания: «⇧⌘K» на macOS, «Ctrl+Shift+K» на остальных (как
 * `Shortcut::label` в Rust). */
export function shortcutLabel(s: string, platform: Platform, layout?: LayoutMap): string {
  const sc = parseShortcut(s);
  if (!sc) return s;
  const key = keyLabel(sc.code, layout);
  if (platform === 'mac') {
    return `${sc.ctrl ? '⌃' : ''}${sc.alt ? '⌥' : ''}${sc.shift ? '⇧' : ''}${sc.meta ? '⌘' : ''}${key}`;
  }
  const mods = [sc.ctrl && 'Ctrl', sc.alt && 'Alt', sc.shift && 'Shift', sc.meta && 'Win'].filter(Boolean);
  return [...mods, key].join('+');
}

/** Текст подсказки: «Название  Клавиша» (два пробела, как в дизайн-системе) или
 * только название, если сочетаний нет. */
export function tipText(name: string, label?: string): string {
  return label ? `${name}  ${label}` : name;
}

/** Сочетание из события клавиатуры (по `code`); `null` — нажат только модификатор. */
export function eventShortcut(e: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string | null {
  if (!e.code || MODIFIER_CODES.has(e.code)) return null;
  return formatShortcut({ code: e.code, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey });
}

/** Сочетание, которое браузер превращает в событие буфера обмена. */
export function clipboardAction(s: string, platform: Platform): 'copy' | 'cut' | 'paste' | null {
  const mod = platform === 'mac' ? 'Meta' : 'Ctrl';
  switch (s) {
    case `${mod}+KeyC`:
      return 'copy';
    case `${mod}+KeyX`:
      return 'cut';
    case `${mod}+KeyV`:
      return 'paste';
    default:
      return null;
  }
}

/** Раскладка клавиатуры пользователя, если браузер её отдаёт (Chromium). */
export async function loadLayoutMap(): Promise<LayoutMap | undefined> {
  const kb = (navigator as Navigator & { keyboard?: { getLayoutMap?: () => Promise<LayoutMap> } }).keyboard;
  try {
    return (await kb?.getLayoutMap?.()) ?? undefined;
  } catch {
    return undefined;
  }
}
