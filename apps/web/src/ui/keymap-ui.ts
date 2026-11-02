
// Таблица раскладки для настроек: группы команд из таблицы ядра (`tool`, `edit`…),
// поиск по названию и подписи, сочетания, которые перехватывает браузер,
// конфликты, запись сочетания с клавиатуры.

import type { ConflictInfo, KeymapEntry } from '../wasm/ferrum_wasm';
import type { Platform } from '../core/core';
import { MAX_KEYMAP_BYTES } from '../core/limits';
import { clipboardAction, eventShortcut, shortcutLabel, type LayoutMap } from './shortcuts';

/** Предел файла раскладки — как у ядра (`MAX_KEYMAP_JSON`). */
export const MAX_KEYMAP_FILE = MAX_KEYMAP_BYTES;

export interface KeymapGroup {
  id: string;
  title: string;
  commands: KeymapEntry[];
}

/** Порядок и названия групп; неизвестная группа — в конце под своим ключом. */
export const GROUPS: { id: string; title: string }[] = [
  { id: 'tool', title: 'Инструменты' },
  { id: 'edit', title: 'Правка' },
  { id: 'object', title: 'Объект' },
  { id: 'text', title: 'Текст' },
  { id: 'vector', title: 'Вектор' },
  { id: 'arrange', title: 'Порядок' },
  { id: 'nudge', title: 'Сдвиг' },
  { id: 'view', title: 'Вид' },
  { id: 'file', title: 'Файл' },
  { id: 'settings', title: 'Настройки' },
  { id: 'help', title: 'Справка' },
];

export function groupCommands(commands: readonly KeymapEntry[]): KeymapGroup[] {
  const out: KeymapGroup[] = GROUPS.map((g) => ({ ...g, commands: [] }));
  for (const c of commands) {
    let g = out.find((x) => x.id === c.group);
    if (!g) {
      g = { id: c.group, title: c.group, commands: [] };
      out.push(g);
    }
    g.commands.push(c);
  }
  return out.filter((g) => g.commands.length > 0);
}

/** Поиск: по названию команды (без учёта регистра) или по подписи сочетания. */
export function filterGroups(groups: KeymapGroup[], query: string, platform: Platform, layout?: LayoutMap): KeymapGroup[] {
  const q = query.trim().toLowerCase();
  if (!q) return groups;
  return groups
    .map((g) => ({
      ...g,
      commands: g.commands.filter(
        (c) =>
          c.name.toLowerCase().includes(q) ||
          c.bindings.some((b) => shortcutLabel(b, platform, layout).toLowerCase() === q),
      ),
    }))
    .filter((g) => g.commands.length > 0);
}

/** Сочетания команды, которые перехватывает браузер. */
export function reservedOf(entry: KeymapEntry, reserved: readonly string[]): string[] {
  return entry.bindings.filter((b) => reserved.includes(b));
}

/** Другие команды на том же сочетании (ключи). */
export function conflictsOf(entry: KeymapEntry, shortcut: string, conflicts: readonly ConflictInfo[]): string[] {
  return conflicts.find((c) => c.shortcut === shortcut)?.commands.filter((k) => k !== entry.key) ?? [];
}

/** Что сделать при нажатии во время записи сочетания. */
export type RecordResult =
  | { kind: 'cancel' }
  | { kind: 'wait' }
  /** AltGr на Windows/Linux: редактор такие сочетания не принимает. */
  | { kind: 'altgr' }
  /** Сочетание, которое не сработает никогда: Tab у браузера, буфер обмена — события copy/cut/paste. */
  | { kind: 'never' }
  | { kind: 'shortcut'; shortcut: string };

type RecordEvent = Pick<KeyboardEvent, 'code' | 'key' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'> &
  Partial<Pick<KeyboardEvent, 'getModifierState'>>;

export function recordKey(e: RecordEvent, platform: Platform = 'windows'): RecordResult {
  // Esc без модификаторов — отмена записи (как и в редакторе, Esc всегда выход).
  if (e.code === 'Escape' && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) return { kind: 'cancel' };
  const s = eventShortcut(e);
  if (!s) return { kind: 'wait' };
  // Договор с редактором (`KeyEvent::alt_graph`): с настоящим AltGr сочетания не
  // срабатывают (на Linux AltGr приходит и без флагов Ctrl/Alt); на macOS AltGraph
  // игнорируется (так приходит Option).
  if (platform !== 'mac' && e.getModifierState?.('AltGraph')) return { kind: 'altgr' };
  // Tab всегда у браузера (keys.ts), Ctrl/Cmd+C, X, V идут событиями буфера обмена.
  if (e.code === 'Tab' || clipboardAction(s, platform)) return { kind: 'never' };
  return { kind: 'shortcut', shortcut: s };
}
