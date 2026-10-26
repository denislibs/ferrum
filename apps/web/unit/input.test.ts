// Одиночный Alt: его отпускание на холсте не должно уводить фокус в меню браузера
// (Windows), а Alt в сочетаниях и автоповтор — не «одиночные».

import { expect, test } from '@playwright/test';
import { createLoneAlt, isAltCode } from '../src/core/input';

const key = (code: string, repeat = false) => ({ code, repeat }) as KeyboardEvent;

test('отпускание одиночного Alt распознаётся, обе клавиши Alt', () => {
  const t = createLoneAlt();
  for (const code of ['AltLeft', 'AltRight']) {
    expect(isAltCode(code)).toBe(true);
    t.down(key(code));
    // Автоповтор удерживаемого Alt одиночности не снимает.
    t.down(key(code, true));
    t.down(key(code, true));
    expect(t.up(key(code))).toBe(true);
    // Второе отпускание без нажатия — уже нет.
    expect(t.up(key(code))).toBe(false);
  }
  expect(isAltCode('ControlLeft')).toBe(false);
  expect(isAltCode('KeyA')).toBe(false);
});

test('Alt в сочетании, отпускание другой клавиши и потеря фокуса — не одиночный', () => {
  const t = createLoneAlt();
  // Ctrl+Alt+G: между нажатием и отпусканием Alt была другая клавиша.
  t.down(key('AltLeft'));
  t.down(key('KeyG'));
  expect(t.up(key('KeyG'))).toBe(false);
  expect(t.up(key('AltLeft'))).toBe(false);
  // Отпускание без нажатия (Alt нажали, пока фокус был в другом окне).
  expect(t.up(key('AltLeft'))).toBe(false);
  // Потеря фокуса окна между нажатием и отпусканием.
  t.down(key('AltLeft'));
  t.reset();
  expect(t.up(key('AltLeft'))).toBe(false);
  // Автоповтор после сочетания не делает Alt снова одиночным.
  t.down(key('AltLeft'));
  t.down(key('KeyD'));
  t.down(key('AltLeft', true));
  expect(t.up(key('AltLeft'))).toBe(false);
});
