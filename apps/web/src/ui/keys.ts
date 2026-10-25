// Какие клавиши оставить интерфейсу, а какие переслать редактору (спека 7.6:
// «если фокус в поле ввода панели, клавиши обрабатывает интерфейс»).

/** Элемент, где клавиши — ввод текста или навигация по виджету. */
function ownsKeys(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  // contenteditable без значения, «true» и «plaintext-only» — всё редактируемое.
  if (t.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return true;
  if (t instanceof HTMLElement && t.isContentEditable) return true;
  // Меню, диалоги, списки: стрелки, Enter, Esc, Tab — их клавиатурная навигация.
  return !!t.closest('[role="menu"], [role="dialog"], [role="listbox"]');
}

/** Клавиши кнопок и вкладок: активировать и переходить фокусом. */
const WIDGET_KEYS = new Set(['Tab', 'Enter', 'Space', 'NumpadEnter']);

const ARROW_ROLES =
  '[role="tab"], [role="treeitem"], [role="menuitem"], [role="option"], [role="slider"], [role="separator"], [role="toolbar"], [role="tablist"]';

/** Пересылать ли клавишу редактору. */
export function forwardKey(e: KeyboardEvent, isClipboardKey: (e: KeyboardEvent) => boolean): boolean {
  if (ownsKeys(e.target)) return false;
  // Tab всегда у браузера: фокус ходит по интерфейсу.
  if (e.code === 'Tab') return false;
  const onControl = e.target instanceof Element && !!e.target.closest('button, [role="tab"], [role="treeitem"], a');
  if (onControl && WIDGET_KEYS.has(e.code)) return false;
  // Стрелки внутри вкладок, деревьев, меню, списков, ползунков, тулбара и краёв панелей — их навигация.
  if (e.code.startsWith('Arrow') && e.target instanceof Element && e.target.closest(ARROW_ROLES)) return false;
  // Ctrl/Cmd+C, X, V идут событиями copy/cut/paste (системный буфер), а не в ядро.
  return !isClipboardKey(e);
}
