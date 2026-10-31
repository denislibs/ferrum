// Клавиши поля правки текста: ядру, браузеру или погасить; виды `beforeinput`;
// длина в UTF-8 для предела команды.

import { expect, test } from '@playwright/test';
import { inputKind, textKeyAction, utf8Length } from '../src/core/text-keys';

const key = (
  code: string,
  mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'isComposing' | 'altGraph', boolean> & { keyCode: number }> = {},
  k = '',
) => ({
  code,
  key: k,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  isComposing: false,
  ...mods,
});

test('ход курсора, удаление и форматирование — ядру', () => {
  expect(textKeyAction(key('ArrowLeft', { shiftKey: true }), 'windows')).toBe('core');
  expect(textKeyAction(key('Backspace', { altKey: true }), 'mac')).toBe('core');
  expect(textKeyAction(key('KeyB', { metaKey: true }), 'mac')).toBe('core');
  expect(textKeyAction(key('KeyX', { ctrlKey: true, shiftKey: true }), 'windows')).toBe('core');
  expect(textKeyAction(key('KeyT', { metaKey: true, altKey: true }), 'mac')).toBe('core');
  expect(textKeyAction(key('KeyY', { ctrlKey: true }), 'linux')).toBe('core');
});

test('набор, буфер обмена и составление — браузеру; команды раскладки гаснут', () => {
  expect(textKeyAction(key('KeyR', {}, 'r'), 'windows')).toBe('browser');
  expect(textKeyAction(key('KeyC', { ctrlKey: true }), 'windows')).toBe('browser');
  expect(textKeyAction(key('ArrowLeft', { isComposing: true }), 'mac')).toBe('browser');
  expect(textKeyAction(key('KeyD', { metaKey: true }), 'mac')).toBe('block');
  expect(textKeyAction(key('KeyG', { ctrlKey: true }), 'windows')).toBe('block');
  // Перезагрузка страницы остаётся браузеру.
  expect(textKeyAction(key('KeyR', { ctrlKey: true }), 'windows')).toBe('browser');
});

test('виды beforeinput', () => {
  expect(inputKind('insertText')).toBe('insert');
  expect(inputKind('insertLineBreak')).toBe('paragraph');
  expect(inputKind('deleteWordBackward')).toBe('deleteWordBackward');
  expect(inputKind('deleteSoftLineBackward')).toBe('deleteLineBackward');
  expect(inputKind('insertCompositionText')).toBeNull();
  expect(inputKind('insertFromPaste')).toBeNull();
});

test('длина в UTF-8', () => {
  expect(utf8Length('abc')).toBe(3);
  expect(utf8Length('я')).toBe(2);
  expect(utf8Length('€')).toBe(3);
  expect(utf8Length('😀')).toBe(4);
});

test('Safari: клавиша после compositionend (keyCode 229, isComposing false) — IME, не ядру', () => {
  // Порядок Safari: compositionend, затем keydown Escape/Backspace с isComposing: false.
  expect(textKeyAction(key('Escape', { keyCode: 229 }, 'Escape'), 'mac')).toBe('browser');
  expect(textKeyAction(key('Backspace', { keyCode: 229 }, 'Backspace'), 'mac')).toBe('browser');
  // Без 229 те же клавиши — ядру.
  expect(textKeyAction(key('Escape', { keyCode: 27 }, 'Escape'), 'mac')).toBe('core');
  expect(textKeyAction(key('Backspace', { keyCode: 8 }, 'Backspace'), 'mac')).toBe('core');
});

test('AltGr (Ctrl+Alt в Windows) — набор символа; выравнивание — без Shift', () => {
  expect(textKeyAction(key('KeyL', { ctrlKey: true, altKey: true, altGraph: true }, 'ł'), 'windows')).toBe('browser');
  expect(textKeyAction(key('KeyL', { ctrlKey: true, altKey: true, shiftKey: true, altGraph: true }, 'Ł'), 'windows')).toBe('browser');
  expect(textKeyAction(key('KeyL', { ctrlKey: true, altKey: true }), 'windows')).toBe('core');
  expect(textKeyAction(key('KeyL', { ctrlKey: true, altKey: true, shiftKey: true }), 'windows')).toBe('browser');
});

test('Ctrl/Cmd+Shift+V — вставка обычным текстом, браузеру', () => {
  expect(textKeyAction(key('KeyV', { ctrlKey: true, shiftKey: true }), 'windows')).toBe('browser');
  expect(textKeyAction(key('KeyV', { metaKey: true, shiftKey: true }), 'mac')).toBe('browser');
});
