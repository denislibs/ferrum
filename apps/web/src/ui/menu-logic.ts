// Чистая логика меню (спека подсистемы 2, раздел 4): где встать у якоря и как
// отразиться у края окна, переход по первой букве.

export interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** Отступ меню от края окна. */
export const EDGE = 8;
/** Задержка открытия и закрытия подменю наведением (мс), как в Figma. */
export const SUBMENU_DELAY_MS = 150;

/** Положение меню размера `size` у якоря в окне `view`.
 * - Корневое меню — под якорем (`above` — над ним); не влезает снизу — над якорем,
 *   справа — сдвигается влево.
 * - Подменю — справа от пункта, верхом вровень с ним; у правого края окна
 *   отражается налево от пункта, у нижнего — поднимается. */
export function placeMenu(
  anchor: Box,
  size: { width: number; height: number },
  view: { width: number; height: number },
  opts: { above?: boolean; submenu?: boolean } = {},
): { x: number; y: number } {
  const { width: w, height: h } = size;
  let x: number;
  let y: number;
  if (opts.submenu) {
    x = anchor.right;
    if (x + w > view.width - EDGE) x = anchor.left - w;
    if (x < EDGE) x = Math.max(EDGE, view.width - EDGE - w);
    // Первый пункт подменю — вровень с пунктом-родителем (отступ меню — 4 px).
    y = anchor.top - 4;
    if (y + h > view.height - EDGE) y = view.height - EDGE - h;
  } else {
    x = anchor.left;
    y = opts.above ? anchor.top - h - 4 : anchor.bottom + 4;
    if (x + w > view.width - EDGE) x = view.width - EDGE - w;
    if (y + h > view.height - EDGE) y = anchor.top - h - 4;
  }
  return { x: Math.max(EDGE, x), y: Math.max(EDGE, y) };
}

/** Переход по первой букве: следующий после `from` пункт, чьё название
 * начинается с `ch` (без учёта регистра и «ё»), по кругу; `-1` — такого нет. */
export function typeAhead(labels: readonly string[], from: number, ch: string): number {
  const norm = (s: string) => s.trim().toLowerCase().replaceAll('ё', 'е');
  const c = norm(ch);
  if (!c) return -1;
  const n = labels.length;
  for (let k = 1; k <= n; k++) {
    const i = (from + k + n) % n;
    if (norm(labels[i]).startsWith(c)) return i;
  }
  return -1;
}
