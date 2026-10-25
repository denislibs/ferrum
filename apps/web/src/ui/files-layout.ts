// Раскладка экрана файлов без DOM: столбцы сетки и шаг строк по ширине области,
// окно записей по прокрутке (виртуальный список просит у worker'а только его).

import { MAX_WINDOW } from '../core/file-list';
import type { FilesView } from './settings';

/** Сетка: зазор, наименьшая ширина карточки, высота подписи под превью. */
export const GRID_GAP = 16;
export const CARD_MIN = 200;
export const CARD_META = 52;
/** Список: высота строки. */
export const LIST_ROW = 48;
/** Запас строк сверху и снизу видимой части. */
const OVERSCAN = 1;

export interface FilesLayout {
  cols: number;
  /** Ширина карточки и высота превью (сетка); в списке — ширина строки. */
  cardW: number;
  thumbH: number;
  /** Шаг строки по вертикали. */
  rowH: number;
}

/** Раскладка по ширине области: столбцы сетки и шаг строк (список — один столбец). */
export function filesLayout(width: number, view: FilesView): FilesLayout {
  const w = Math.max(0, width);
  if (view === 'list') return { cols: 1, cardW: w, thumbH: 32, rowH: LIST_ROW };
  const cols = Math.max(1, Math.floor((w + GRID_GAP) / (CARD_MIN + GRID_GAP)));
  const cardW = Math.max(CARD_MIN, (w - GRID_GAP * (cols - 1)) / cols);
  const thumbH = Math.round((cardW * 2) / 3);
  return { cols, cardW, thumbH, rowH: thumbH + CARD_META + GRID_GAP };
}

/** Окно записей для прокрутки: первые и последние видимые строки с запасом. */
export function filesWindow(scrollTop: number, height: number, l: FilesLayout): { start: number; count: number } {
  const first = Math.max(0, Math.floor(scrollTop / l.rowH) - OVERSCAN);
  const last = Math.ceil((scrollTop + Math.max(height, l.rowH)) / l.rowH) + OVERSCAN;
  return { start: first * l.cols, count: Math.min(MAX_WINDOW, (last - first) * l.cols) };
}

