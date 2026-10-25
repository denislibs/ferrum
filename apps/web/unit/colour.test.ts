// Цвет: HEX ↔ RGB ↔ HSB и обратно без потерь, разбор HEX из поля.

import { expect, test } from '@playwright/test';
import { hsbToRgb, paintOpacity, parseHex, parseOpacityText, rgbToHsb, toHex, toHex6 } from '../src/ui/colour';

test('разбор HEX во всех записях', () => {
  expect(parseHex('#F2814A')).toEqual({ r: 242, g: 129, b: 74, a: 255 });
  expect(parseHex('f2814a80')).toEqual({ r: 242, g: 129, b: 74, a: 128 });
  expect(parseHex(' #f84 ')).toEqual({ r: 255, g: 136, b: 68, a: 255 });
  expect(parseHex('f848')).toEqual({ r: 255, g: 136, b: 68, a: 136 });
  for (const bad of ['', '#', 'ggg', '12345', '#1234567', 'красный']) expect(parseHex(bad), bad).toBeNull();
});

test('HEX документа и поля', () => {
  expect(toHex({ r: 242, g: 129, b: 74, a: 255 })).toBe('#f2814aff');
  expect(toHex6({ r: 1, g: 2, b: 255 })).toBe('0102FF');
  expect(toHex({ r: 300, g: -4, b: 12.6, a: 0 })).toBe('#ff000d00');
});

test('RGB → HSB: опорные цвета', () => {
  expect(rgbToHsb({ r: 255, g: 0, b: 0 })).toEqual({ h: 0, s: 1, b: 1 });
  expect(rgbToHsb({ r: 0, g: 255, b: 0 })).toEqual({ h: 120, s: 1, b: 1 });
  expect(rgbToHsb({ r: 0, g: 0, b: 255 })).toEqual({ h: 240, s: 1, b: 1 });
  expect(rgbToHsb({ r: 0, g: 0, b: 0 })).toEqual({ h: 0, s: 0, b: 0 });
  expect(rgbToHsb({ r: 255, g: 255, b: 255 })).toEqual({ h: 0, s: 0, b: 1 });
  const g = rgbToHsb({ r: 128, g: 128, b: 128 });
  expect([g.h, g.s]).toEqual([0, 0]);
  expect(hsbToRgb({ h: 360, s: 1, b: 1 })).toEqual({ r: 255, g: 0, b: 0 });
  expect(hsbToRgb({ h: -60, s: 1, b: 1 })).toEqual({ r: 255, g: 0, b: 255 });
});

test('RGB → HSB → RGB и HEX → RGB → HEX без потерь', () => {
  let n = 0;
  const lost: string[] = [];
  for (let r = 0; r <= 255; r += 15) {
    for (let g = 0; g <= 255; g += 17) {
      for (let b = 0; b <= 255; b += 5) {
        const c = { r, g, b };
        const back = hsbToRgb(rgbToHsb(c));
        const hex = toHex({ ...c, a: 255 });
        if (back.r !== r || back.g !== g || back.b !== b || toHex(parseHex(hex)!) !== hex) lost.push(hex);
        n++;
      }
    }
  }
  expect(lost).toEqual([]);
  expect(n).toBe(18 * 16 * 52);
});

test('прозрачность в панели — альфа цвета × прозрачность заливки', () => {
  expect(paintOpacity({ r: 0, g: 0, b: 0, a: 255 }, 0.5)).toBe(0.5);
  expect(paintOpacity({ r: 0, g: 0, b: 0, a: 51 }, 1)).toBeCloseTo(0.2);
});

test('прозрачность из поля: проценты, выражения, пределы', () => {
  expect(parseOpacityText('50', 100)).toBe(0.5);
  expect(parseOpacityText('50%', 100)).toBe(0.5);
  expect(parseOpacityText('30+20', 100)).toBe(0.5);
  expect(parseOpacityText('*2', 30)).toBe(0.6);
  expect(parseOpacityText('250', 100)).toBe(1);
  expect(parseOpacityText('-5', 100)).toBe(0);
  expect(parseOpacityText('33.4', 100)).toBe(0.33);
  expect(parseOpacityText('abc', 100)).toBeNull();
  expect(parseOpacityText('', 100)).toBeNull();
});
