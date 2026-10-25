// Сочетания в интерфейсе: та же каноническая запись и те же подписи, что у ядра
// (`Shortcut::to_string`, `Shortcut::label`), сочетание из события — по
// физической клавише, сочетания буфера обмена.

import { expect, test } from '@playwright/test';
import {
  clipboardAction,
  eventShortcut,
  formatShortcut,
  keyLabel,
  parseShortcut,
  shortcutLabel,
  tipText,
} from '../src/ui/shortcuts';

test('разбор и каноническая запись', () => {
  expect(parseShortcut('Shift+Ctrl+KeyZ')).toEqual({ code: 'KeyZ', ctrl: true, alt: false, shift: true, meta: false });
  expect(formatShortcut(parseShortcut('Meta+Shift+Alt+Ctrl+KeyK')!)).toBe('Ctrl+Alt+Shift+Meta+KeyK');
  expect(parseShortcut('Cmd+KeyC')?.meta).toBe(true);
  expect(parseShortcut('Ctrl+Ctrl+KeyC')).toBeNull();
  expect(parseShortcut('Hyper+KeyC')).toBeNull();
  expect(parseShortcut('')).toBeNull();
});

test('подписи как у ядра: macOS и Windows/Linux', () => {
  // Пары из `Shortcut::label` (crates/ferrum-editor/src/keymap.rs).
  expect(shortcutLabel('Shift+Meta+KeyK', 'mac')).toBe('⇧⌘K');
  expect(shortcutLabel('Ctrl+Shift+KeyK', 'windows')).toBe('Ctrl+Shift+K');
  expect(shortcutLabel('Alt+Meta+BracketRight', 'mac')).toBe('⌥⌘]');
  expect(shortcutLabel('Ctrl+Backslash', 'linux')).toBe('Ctrl+\\');
  expect(shortcutLabel('Shift+Digit1', 'windows')).toBe('Shift+1');
  expect(shortcutLabel('Escape', 'mac')).toBe('Esc');
  expect(shortcutLabel('Shift+ArrowLeft', 'linux')).toBe('Shift+←');
  expect(shortcutLabel('Meta+KeyZ', 'windows')).toBe('Win+Z');
  expect(shortcutLabel('Delete', 'windows')).toBe('Delete');
});

test('подпись по раскладке пользователя', () => {
  const ru = new Map([
    ['KeyR', 'к'],
    ['BracketLeft', 'х'],
  ]);
  expect(keyLabel('KeyR', ru)).toBe('К');
  expect(shortcutLabel('Ctrl+BracketLeft', 'windows', ru)).toBe('Ctrl+Х');
  expect(keyLabel('ArrowUp', ru)).toBe('↑');
  // Нет в раскладке — подпись US.
  expect(keyLabel('KeyQ', ru)).toBe('Q');
});

test('подсказка «Название  Клавиша»', () => {
  expect(tipText('Прямоугольник', 'R')).toBe('Прямоугольник  R');
  expect(tipText('Поделиться')).toBe('Поделиться');
});

test('сочетание из события — по физической клавише', () => {
  const ev = (code: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {}) => ({
    code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  });
  // Русская раскладка: «к» — та же клавиша KeyR.
  expect(eventShortcut(ev('KeyR'))).toBe('KeyR');
  expect(eventShortcut(ev('KeyZ', { metaKey: true, shiftKey: true }))).toBe('Shift+Meta+KeyZ');
  expect(eventShortcut(ev('ShiftLeft', { shiftKey: true }))).toBeNull();
  expect(eventShortcut(ev(''))).toBeNull();
});

test('сочетания буфера обмена по платформе', () => {
  expect(clipboardAction('Meta+KeyC', 'mac')).toBe('copy');
  expect(clipboardAction('Ctrl+KeyC', 'mac')).toBeNull();
  expect(clipboardAction('Ctrl+KeyX', 'windows')).toBe('cut');
  expect(clipboardAction('Ctrl+KeyV', 'linux')).toBe('paste');
  expect(clipboardAction('Ctrl+Shift+KeyC', 'linux')).toBeNull();
});
