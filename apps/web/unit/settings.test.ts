
// Настройки и таблица раскладки: чтение с умолчаниями при мусоре и отказе
// хранилища, тема по системе, группы и поиск команд, браузерные сочетания,
// конфликты, запись сочетания.

import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import type { KeymapEntry } from '../src/wasm/ferrum_wasm';
import { createUiStore } from '../src/ui/store';
import { shortcutLabel } from '../src/ui/shortcuts';
import { conflictsOf, filterGroups, groupCommands, recordKey, reservedOf } from '../src/ui/keymap-ui';
import {
  DEFAULT_SETTINGS,
  loadSettings,
  PANEL_MAX,
  PANEL_MIN,
  resolveTheme,
  saveSettings,
  SETTINGS_KEY,
  THEME_CANVAS,
} from '../src/ui/settings';

function memory(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, v),
    removeItem: (k) => void m.delete(k),
    clear: () => m.clear(),
    key: () => null,
    length: 0,
  };
}

test('настройки: запись и чтение, мусор и отказ хранилища', () => {
  const s = memory();
  expect(loadSettings(s)).toEqual(DEFAULT_SETTINGS);
  const mine = { ...DEFAULT_SETTINGS, theme: 'light' as const, pixelSnap: false, keymap: '{"x":1}', leftWidth: 300 };
  expect(saveSettings(s, mine)).toBe(true);
  expect(loadSettings(s)).toEqual(mine);
  s.setItem(SETTINGS_KEY, '{не json');
  expect(loadSettings(s)).toEqual(DEFAULT_SETTINGS);
  s.setItem(SETTINGS_KEY, JSON.stringify({ theme: 'синяя', pixelSnap: 'да', leftWidth: 'широко', pagesOpen: false }));
  expect(loadSettings(s)).toEqual({ ...DEFAULT_SETTINGS, pagesOpen: false });
  const broken = {
    getItem: () => {
      throw new Error('SecurityError');
    },
    setItem: () => {
      throw new Error('QuotaExceededError');
    },
  };
  expect(loadSettings(broken)).toEqual(DEFAULT_SETTINGS);
  expect(saveSettings(broken, mine)).toBe(false);
  expect(loadSettings(null)).toEqual(DEFAULT_SETTINGS);
});

test('тема «системная» — по prefers-color-scheme', () => {
  expect(resolveTheme('system', true)).toBe('dark');
  expect(resolveTheme('system', false)).toBe('light');
  expect(resolveTheme('light', true)).toBe('light');
});

const entry = (key: string, name: string, bindings: string[]): KeymapEntry => ({
  key,
  name,
  group: key.split('.')[0],
  bindings,
  labels: [],
  defaults: bindings,
  overridden: false,
});

test('группы, поиск, браузерные сочетания и конфликты', () => {
  const commands = [
    entry('view.zoom_in', 'Приблизить', ['Equal']),
    entry('tool.rect', 'Прямоугольник', ['KeyR']),
    entry('tool.ellipse', 'Эллипс', ['KeyR', 'KeyO']),
    entry('edit.duplicate', 'Дублировать', ['Ctrl+KeyD', 'Ctrl+KeyW']),
    entry('plugin.run', 'Плагин', []),
  ];
  const groups = groupCommands(commands);
  expect(groups.map((g) => g.title)).toEqual(['Инструменты', 'Правка', 'Вид', 'plugin']);
  expect(filterGroups(groups, 'ЭЛЛ', 'windows')[0].commands.map((c) => c.key)).toEqual(['tool.ellipse']);
  expect(filterGroups(groups, 'r', 'windows')[0].commands.map((c) => c.key)).toEqual(['tool.rect', 'tool.ellipse']);
  expect(filterGroups(groups, 'ctrl+d', 'windows').map((g) => g.id)).toEqual(['edit']);
  expect(reservedOf(commands[3], ['Ctrl+KeyW', 'Ctrl+KeyT'])).toEqual(['Ctrl+KeyW']);
  const conflicts = [{ shortcut: 'KeyR', label: 'R', commands: ['tool.rect', 'tool.ellipse'] }];
  expect(conflictsOf(commands[1], 'KeyR', conflicts)).toEqual(['tool.ellipse']);
  expect(conflictsOf(commands[1], 'KeyO', conflicts)).toEqual([]);
});

test('запись сочетания: Esc — отмена, модификатор — ждать, клавиша — сочетание', () => {
  const k = (code: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}) => ({
    code,
    key: '',
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });
  expect(recordKey(k('Escape'))).toEqual({ kind: 'cancel' });
  expect(recordKey(k('Escape', { shiftKey: true }))).toEqual({ kind: 'shortcut', shortcut: 'Shift+Escape' });
  expect(recordKey(k('ShiftLeft', { shiftKey: true }))).toEqual({ kind: 'wait' });
  // Русская раскладка: «й» — физическая KeyQ.
  expect(recordKey({ ...k('KeyQ', { ctrlKey: true }), key: 'й' })).toEqual({ kind: 'shortcut', shortcut: 'Ctrl+KeyQ' });
});

test('запись сочетания: AltGr на Windows/Linux отклоняется, на macOS игнорируется', () => {
  const altGr = (on: boolean) => ({
    code: 'KeyQ',
    key: '@',
    ctrlKey: true,
    altKey: true,
    shiftKey: false,
    metaKey: false,
    getModifierState: (m: string) => on && m === 'AltGraph',
  });
  expect(recordKey(altGr(true), 'windows')).toEqual({ kind: 'altgr' });
  expect(recordKey(altGr(true), 'linux')).toEqual({ kind: 'altgr' });
  expect(recordKey(altGr(true), 'mac')).toEqual({ kind: 'shortcut', shortcut: 'Ctrl+Alt+KeyQ' });
  // Настоящее Ctrl+Alt без AltGr — обычное сочетание.
  expect(recordKey(altGr(false), 'windows')).toEqual({ kind: 'shortcut', shortcut: 'Ctrl+Alt+KeyQ' });
  // Linux/Windows: AltGraph при любых флагах Ctrl/Alt — тоже AltGr.
  expect(recordKey({ ...altGr(true), ctrlKey: false, altKey: false }, 'linux')).toEqual({ kind: 'altgr' });
  expect(recordKey({ ...altGr(true), ctrlKey: false }, 'windows')).toEqual({ kind: 'altgr' });
  expect(recordKey({ ...altGr(true), ctrlKey: false, altKey: false }, 'mac')).toEqual({ kind: 'shortcut', shortcut: 'KeyQ' });
});

test('запись сочетания: Tab и буфер обмена не сработают никогда', () => {
  const k = (code: string, mods: object = {}) => ({ code, key: '', ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods });
  expect(recordKey(k('Tab'), 'windows')).toEqual({ kind: 'never' });
  expect(recordKey(k('Tab', { shiftKey: true }), 'mac')).toEqual({ kind: 'never' });
  expect(recordKey(k('KeyC', { ctrlKey: true }), 'windows')).toEqual({ kind: 'never' });
  expect(recordKey(k('KeyX', { metaKey: true }), 'mac')).toEqual({ kind: 'never' });
  expect(recordKey(k('KeyV', { metaKey: true }), 'mac')).toEqual({ kind: 'never' });
  // На macOS Ctrl+C — обычное сочетание, на Windows Meta+C — тоже.
  expect(recordKey(k('KeyC', { ctrlKey: true }), 'mac')).toEqual({ kind: 'shortcut', shortcut: 'Ctrl+KeyC' });
  expect(recordKey(k('KeyC', { metaKey: true }), 'windows')).toEqual({ kind: 'shortcut', shortcut: 'Meta+KeyC' });
});

test('сохранённая раскладка при запуске: чужая ошибка её не стирает, отказ ядра — стирает', () => {
  const store = createUiStore();
  let failed = 0;
  store.quietKeymapImport(undefined, () => failed++);
  store.apply({ type: 'error', message: 'Что-то другое пошло не так' });
  expect(failed).toBe(0);
  store.apply({ type: 'error', message: 'Не удалось импортировать раскладку: это не файл' });
  expect(failed).toBe(1);
  // Успех — тихий (без уведомления), флаг снят.
  store.quietKeymapImport();
  store.apply({ type: 'keymapImported', warnings: [] });
  expect(store.state.toasts.filter((t) => t.text.includes('Раскладка импортирована'))).toHaveLength(0);
  store.apply({ type: 'keymapImported', warnings: [] });
  expect(store.state.toasts.some((t) => t.text === 'Раскладка импортирована')).toBe(true);
});

test('русская раскладка: поиск и подписи идут по раскладке, запись — по физической клавише', () => {
  const ru = new Map([
    ['KeyR', 'к'],
    ['KeyQ', 'й'],
  ]);
  const commands = [entry('tool.rect', 'Прямоугольник', ['KeyR']), entry('tool.move', 'Перемещение', ['KeyV'])];
  const groups = groupCommands(commands);
  expect(filterGroups(groups, 'к', 'windows', ru)[0].commands.map((c) => c.key)).toEqual(['tool.rect']);
  expect(filterGroups(groups, 'r', 'windows', ru)).toEqual([]);
  expect(shortcutLabel('KeyQ', 'windows', ru)).toBe('Й');
});

test('холст темы: одно значение в интерфейсе, токенах и ядре', () => {
  const css = readFileSync(new URL('../src/ui/styles/theme.css', import.meta.url), 'utf8');
  const rs = readFileSync(new URL('../../../crates/ferrum-wasm/src/core.rs', import.meta.url), 'utf8');
  const hex = (r: number, g: number, b: number) => `#${[r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')}ff`;
  for (const theme of ['dark', 'light'] as const) {
    const block = css.slice(css.indexOf(theme === 'dark' ? ":root[data-theme='dark']" : ":root[data-theme='light']"));
    expect(block.match(/--canvas:\s*(#[0-9a-f]{6});/)![1] + 'ff').toBe(THEME_CANVAS[theme]);
    const m = new RegExp(`CANVAS_${theme.toUpperCase()}: Rgba = Rgba::new\\(\\[(\\d+)\\.0 / 255\\.0, (\\d+)\\.0 / 255\\.0, (\\d+)\\.0 / 255\\.0, 1\\.0\\]\\)`).exec(rs)!;
    expect(hex(+m[1], +m[2], +m[3])).toBe(THEME_CANVAS[theme]);
  }
});

test('цвета оверлеев холста: те же токены дизайн-системы в интерфейсе и в ядре', () => {
  // Оверлеи рисует ядро (OverlayTheme в ferrum-render), интерфейс передаёт ему только
  // тему; значения должны совпадать с токенами theme.css. `measure` — измерения с Alt.
  const css = readFileSync(new URL('../src/ui/styles/theme.css', import.meta.url), 'utf8');
  const rs = readFileSync(new URL('../../../crates/ferrum-render/src/overlay.rs', import.meta.url), 'utf8');
  const tokens = { select: 'select', on_select: 'on-select', component: 'component', frame_label: 'ink-muted', measure: 'danger' };
  for (const theme of ['dark', 'light'] as const) {
    const block = css.slice(css.indexOf(theme === 'dark' ? ":root[data-theme='dark']" : ":root[data-theme='light']"));
    const fn = rs.slice(rs.indexOf(`pub fn ${theme}() -> OverlayTheme`));
    const body = fn.slice(0, fn.indexOf('\n    }'));
    for (const [field, token] of Object.entries(tokens)) {
      const want = block.match(new RegExp(`--${token}:\\s*#([0-9a-f]{6});`))?.[1];
      const got = body.match(new RegExp(`\\b${field}: hex\\(0x([0-9a-f]{6})\\)`))?.[1];
      expect(want, `${theme}: --${token} в theme.css`).toBeDefined();
      expect(got, `${theme}: ${field} в OverlayTheme`).toBe(want);
    }
  }
});

test('ширина панелей из хранилища зажата в пределах края панели', () => {
  const s = memory();
  for (const [stored, loaded] of [
    [120, PANEL_MIN],
    [199.6, PANEL_MIN],
    [700, PANEL_MAX],
    [1e9, PANEL_MAX],
    [301.4, 301],
  ] as const) {
    s.setItem(SETTINGS_KEY, JSON.stringify({ leftWidth: stored, rightWidth: stored }));
    const got = loadSettings(s);
    expect([got.leftWidth, got.rightWidth], String(stored)).toEqual([loaded, loaded]);
  }
});
