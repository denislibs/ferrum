// Секция «Текст» панели свойств (спека подсистемы 3, раздел 7) — чистая логика:
// разбор и вид интерлиньяжа и трекинга, группы и поиск выбора шрифта,
// начертание при смене семейства. Сводку стиля готовит ядро.

import type { FontChoice, SpacingDto } from '../wasm/ferrum_wasm';
import { defaultStyle } from '../core/fonts';
import { MAX_FONT_SIZE, MAX_LETTER_SPACING_PERCENT, MAX_LINE_HEIGHT_PERCENT } from '../core/limits';
import { clamp, formatNumber, type FieldInput } from './number';

/** Пределы числовых полей секции — те же, что проверяет ядро (оно отклоняет, не зажимает). */
export const SIZE_RANGE = [1, MAX_FONT_SIZE] as const;
export const PARAGRAPH_SPACING_RANGE = [0, MAX_FONT_SIZE] as const;
export const PARAGRAPH_INDENT_RANGE = [-MAX_FONT_SIZE, MAX_FONT_SIZE] as const;

/** Значение, которое уходит ядру: набранное или прибавка к показанному («Смешанные»), в пределах. */
export function fieldValue(input: FieldInput, shown: number, lo: number, hi: number): number {
  return clamp(input.kind === 'set' ? input.value : shown + input.delta, lo, hi);
}

/** Интерлиньяж (`lineHeight`) или трекинг в пределах ядра: пиксели ±10 000 (интерлиньяж от 0),
 * проценты 0…10 000 у интерлиньяжа и ±1000 у трекинга. */
export function clampSpacing(v: SpacingDto, lineHeight: boolean): SpacingDto {
  if (v.unit === 'auto') return v;
  const pct = lineHeight ? MAX_LINE_HEIGHT_PERCENT : MAX_LETTER_SPACING_PERCENT;
  const max = v.unit === 'percent' ? pct : MAX_FONT_SIZE;
  return { unit: v.unit, value: clamp(v.value, lineHeight ? 0 : -max, max) };
}

/** Интерлиньяж или трекинг в поле: «Авто», «150%», «20». Пусто или «авто» —
 * авто (только интерлиньяж, он же `allowAuto`); число без знака — пиксели. Значение вне
 * пределов ядра зажимается (`clampSpacing`). `null` — не разобрать. */
export function parseSpacing(text: string, allowAuto: boolean): SpacingDto | null {
  const t = text.trim().toLowerCase().replace(',', '.');
  if (allowAuto && (t === '' || t === 'авто' || t === 'auto')) return { unit: 'auto', value: 0 };
  const m = /^(-?\d+(?:\.\d+)?)\s*(%|px)?$/.exec(t);
  if (!m) return null;
  const value = Number(m[1]);
  if (!Number.isFinite(value)) return null;
  return clampSpacing({ unit: m[2] === '%' ? 'percent' : 'px', value }, allowAuto);
}

export function formatSpacing(v: SpacingDto): string {
  switch (v.unit) {
    case 'auto':
      return 'Авто';
    case 'percent':
      return `${formatNumber(v.value)}%`;
    case 'px':
      return formatNumber(v.value);
  }
}

/** Строка выбора шрифта: заголовок группы или семейство. */
export type FontRow =
  | { kind: 'heading'; label: string }
  | { kind: 'font'; family: string; source: 'bundled' | 'system' | 'document' };

/** Группы «В документе» / «Вшитые» / «Системные» с поиском по имени (без регистра). */
export function fontRows(catalog: readonly FontChoice[], pageFonts: readonly string[], query: string): FontRow[] {
  const q = query.trim().toLowerCase();
  const hit = (f: string) => q === '' || f.toLowerCase().includes(q);
  const sorted = [...catalog].sort((a, b) => a.family.localeCompare(b.family));
  const groups: [string, FontRow[]][] = [
    ['В документе', [...pageFonts].sort((a, b) => a.localeCompare(b)).filter(hit).map((family) => ({ kind: 'font', family, source: 'document' }))],
    ['Вшитые', sorted.filter((c) => c.source === 'bundled' && hit(c.family)).map((c) => ({ kind: 'font', family: c.family, source: 'bundled' }))],
    ['Системные', sorted.filter((c) => c.source === 'system' && hit(c.family)).map((c) => ({ kind: 'font', family: c.family, source: 'system' }))],
  ];
  const out: FontRow[] = [];
  for (const [label, rows] of groups) {
    if (rows.length === 0) continue;
    out.push({ kind: 'heading', label }, ...rows);
  }
  return out;
}

/** Начертание при смене семейства: то же имя, иначе Regular, иначе первое. */
export function styleFor(catalog: readonly FontChoice[], family: string, current: string): string {
  return defaultStyle(
    catalog.find((c) => c.family === family),
    current,
  ) || 'Regular';
}

/** Окно строк виртуального списка: первая и сколько (с запасом сверху и снизу). */
export function rowWindow(scrollTop: number, height: number, row: number, total: number): { start: number; end: number } {
  const overscan = 6;
  const start = Math.max(0, Math.floor(scrollTop / row) - overscan);
  return { start, end: Math.min(total, start + Math.ceil(height / row) + 2 * overscan) };
}
