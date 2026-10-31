// Клавиши поля правки текста: ядру, браузеру или погасить; виды `beforeinput`;
// длина в UTF-8 для предела команды.

import { expect, test } from '@playwright/test';
import { inputKind, textKeyAction, utf8Length } from '../src/core/text-keys';

const key = (code: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'isComposing', boolean>> = {}, k = '') => ({
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
