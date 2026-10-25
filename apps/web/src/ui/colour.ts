// Цвет для палитры: HEX ↔ RGB ↔ HSB. В документе цвет — `#rrggbbaa`, у заливки
// ещё своя прозрачность; панель показывает HEX без альфы и одну прозрачность
// (альфа цвета × прозрачность заливки), как в Figma.

import { parseFieldInput } from './number';

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface Rgba extends Rgb {
  /** 0–255. */
  a: number;
}

/** Тон 0–360, насыщенность и яркость 0–1. */
export interface Hsb {
  h: number;
  s: number;
  b: number;
}

const byte = (v: number) => Math.min(255, Math.max(0, Math.round(v)));
const hex2 = (v: number) => byte(v).toString(16).padStart(2, '0');

/** Разбор HEX: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, с решёткой и без. */
export function parseHex(text: string): Rgba | null {
  let h = text.trim().replace(/^#/, '');
  if (!/^[0-9a-f]+$/i.test(h)) return null;
  if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join('');
  if (h.length !== 6 && h.length !== 8) return null;
  const n = (i: number) => parseInt(h.slice(i, i + 2), 16);
  return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) : 255 };
}

/** `#rrggbbaa` для документа. */
export function toHex(c: Rgba): string {
  return `#${hex2(c.r)}${hex2(c.g)}${hex2(c.b)}${hex2(c.a)}`;
}

/** HEX без альфы для поля панели: `F2814A`. */
export function toHex6(c: Rgb): string {
  return `${hex2(c.r)}${hex2(c.g)}${hex2(c.b)}`.toUpperCase();
}

export function rgbToHsb({ r, g, b }: Rgb): Hsb {
  const R = r / 255;
  const G = g / 255;
  const B = b / 255;
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === R) h = ((G - B) / d) % 6;
    else if (max === G) h = (B - R) / d + 2;
    else h = (R - G) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : d / max, b: max };
}

export function hsbToRgb({ h, s, b }: Hsb): Rgb {
  const hh = (((h % 360) + 360) % 360) / 60;
  const c = b * s;
  const x = c * (1 - Math.abs((hh % 2) - 1));
  const m = b - c;
  const [r, g, bl] =
    hh < 1 ? [c, x, 0] : hh < 2 ? [x, c, 0] : hh < 3 ? [0, c, x] : hh < 4 ? [0, x, c] : hh < 5 ? [x, 0, c] : [c, 0, x];
  return { r: byte((r + m) * 255), g: byte((g + m) * 255), b: byte((bl + m) * 255) };
}

/** CSS-цвет для свотча. */
export function cssColor(c: Rgb, alpha = 1): string {
  return `rgb(${c.r} ${c.g} ${c.b} / ${alpha})`;
}

/** Прозрачность для панели: альфа цвета × прозрачность заливки, 0–1. */
export function paintOpacity(color: Rgba, opacity: number): number {
  return (color.a / 255) * opacity;
}

/** Прозрачность из поля (проценты: `50`, `50%`, `30+20`) → 0–1, с округлением до
 * процента и пределами 0–100. `null` — не число. */
export function parseOpacityText(text: string, currentPercent: number): number | null {
  const p = parseFieldInput(text, { current: currentPercent, percentOf: 100 });
  if (p?.kind !== 'set') return null;
  return Math.round(Math.min(100, Math.max(0, p.value))) / 100;
}
