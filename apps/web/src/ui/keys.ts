// Какие клавиши оставить интерфейсу, а какие переслать редактору (спека 7.6:
// «если фокус в поле ввода панели, клавиши обрабатывает интерфейс»).

/** Поле ввода текста: клавиши и события буфера обмена в нём — обычные, браузера. */
export function editableTarget(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  // contenteditable без значения, «true» и «plaintext-only» — всё редактируемое.
  if (t.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return true;
  return t instanceof HTMLElement && t.isContentEditable;
}

/** Открыт модальный диалог: холст не получает ни клавиш, ни буфера обмена. */
export function modalOpen(): boolean {
  return !!document.querySelector('[role="dialog"][aria-modal="true"]');
}

/** Элемент, где клавиши — ввод текста или навигация по виджету. */
function ownsKeys(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  if (editableTarget(t)) return true;
  // Меню, диалоги, списки: стрелки, Enter, Esc, Tab — их клавиатурная навигация.
  return !!t.closest('[role="menu"], [role="dialog"], [role="listbox"]');
}

/** Навигация и удаление: на холсте они двигают и удаляют выделение. Элемент
 * управления (кнопка, переключатель, вкладка, строка, ползунок, край панели)
 * держит их у себя; контейнеры (панель, корень дерева, тулбар) — пропускают. */
const NAV_KEYS = new Set([
  'ArrowLeft',
  'ArrowRight',
  'ArrowUp',
  'ArrowDown',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'Delete',
  'Backspace',
]);

const CONTROLS =
  'button, select, [role="radio"], [role="tab"], [role="option"], [role="treeitem"], [role="slider"], [role="separator"], [role="menuitem"], [role="menuitemcheckbox"]';

/** Откуда пришёл фокус: элемент, получивший его щелчком (нажатие указателя без
 * клавиш после него). `:focus-visible` для этого не годится — Chromium включает его
 * у кнопки, как только на ней нажали клавишу. */
let pointerFocus: Element | null = null;
let pointerActive = false;
if (typeof document !== 'undefined') {
  document.addEventListener('pointerdown', () => (pointerActive = true), true);
  document.addEventListener('keydown', () => (pointerActive = false), true);
  document.addEventListener('focusin', (e) => (pointerFocus = pointerActive && e.target instanceof Element ? e.target : null), true);
}

/** Клавишу навигации или удаления держит элемент управления в фокусе. Кнопку и
 * переключатель — только если до них дошли с клавиатуры: после щелчка мышью
 * Delete удаляет выделение, как в Figma. */
function controlOwnsNav(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  if (editableTarget(t)) return true;
  if (!t.matches(CONTROLS)) return false;
  // Обычная кнопка (без своей роли) и переключатель: только с клавиатуры.
  if (t.matches('button:not([role]), [role="radio"]')) return t !== pointerFocus;
  return true;
}

/** Клавиши кнопок и вкладок: активировать и переходить фокусом. */
const WIDGET_KEYS = new Set(['Tab', 'Enter', 'Space', 'NumpadEnter']);
/** Строки дерева слоёв: ещё F2 (переименовать); стрелки — ниже, у ARROW_ROLES. */
const TREE_KEYS = new Set([...WIDGET_KEYS, 'F2']);

const ARROW_ROLES =
  '[role="tab"], [role="treeitem"], [role="menuitem"], [role="option"], [role="slider"], [role="separator"]';

/** Пересылать ли клавишу редактору. */
export function forwardKey(e: KeyboardEvent, isClipboardKey: (e: KeyboardEvent) => boolean): boolean {
  if (ownsKeys(e.target)) return false;
  // Открыт модальный диалог: холст клавиш не получает, даже если фокус потерялся.
  if (modalOpen()) return false;
  // Tab всегда у браузера: фокус ходит по интерфейсу.
  if (e.code === 'Tab') return false;
  // Стрелки, Home/End, Delete и Backspace на элементе управления, до которого дошли
  // с клавиатуры, не сдвигают и не удаляют выделение. Удаление из дерева слоёв — его
  // же обработчиком (та же команда `edit.delete`), сюда клавиша не доходит.
  if ((NAV_KEYS.has(e.code) || NAV_KEYS.has(e.key)) && controlOwnsNav(e.target)) return false;
  const el = e.target instanceof Element ? e.target : null;
  if (el?.closest('[role="treeitem"]') && TREE_KEYS.has(e.code)) return false;
  if (el?.closest('button, [role="tab"], a') && WIDGET_KEYS.has(e.code)) return false;
  // Стрелки на вкладке, строке, пункте меню и списка, ползунке и крае панели — их навигация.
  if (e.code.startsWith('Arrow') && e.target instanceof Element && e.target.closest(ARROW_ROLES)) return false;
  // Ctrl/Cmd+C, X, V идут событиями copy/cut/paste (системный буфер), а не в ядро.
  return !isClipboardKey(e);
}
