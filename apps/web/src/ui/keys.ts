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

/** Навигация и удаление: на холсте они двигают и удаляют выделение, а на любом
 * элементе интерфейса — его собственная работа (или ничего). Холсту они идут,
 * только если фокус на холсте или ни на чём (body). */
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

/** Фокус на холсте или нигде: клавиши навигации — редактору. */
function canvasOrBody(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return true;
  return t === document.body || t === document.documentElement || t instanceof HTMLCanvasElement;
}

/** Клавиши кнопок и вкладок: активировать и переходить фокусом. */
const WIDGET_KEYS = new Set(['Tab', 'Enter', 'Space', 'NumpadEnter']);
/** Строки дерева слоёв: ещё F2 (переименовать); стрелки — ниже, у ARROW_ROLES. */
const TREE_KEYS = new Set([...WIDGET_KEYS, 'F2']);

const ARROW_ROLES =
  '[role="tab"], [role="tree"], [role="treeitem"], [role="menuitem"], [role="option"], [role="slider"], [role="separator"], [role="toolbar"], [role="tablist"]';

/** Пересылать ли клавишу редактору. */
export function forwardKey(e: KeyboardEvent, isClipboardKey: (e: KeyboardEvent) => boolean): boolean {
  if (ownsKeys(e.target)) return false;
  // Открыт модальный диалог: холст клавиш не получает, даже если фокус потерялся.
  if (modalOpen()) return false;
  // Tab всегда у браузера: фокус ходит по интерфейсу.
  if (e.code === 'Tab') return false;
  // Стрелки, Home/End, Delete и Backspace на кнопке, свотче, переключателе панели
  // не сдвигают и не удаляют выделение. Удаление из дерева слоёв — его же
  // обработчиком (та же команда `edit.delete`), сюда клавиша не доходит.
  if ((NAV_KEYS.has(e.code) || NAV_KEYS.has(e.key)) && !canvasOrBody(e.target)) return false;
  const el = e.target instanceof Element ? e.target : null;
  if (el?.closest('[role="treeitem"]') && TREE_KEYS.has(e.code)) return false;
  if (el?.closest('button, [role="tab"], a') && WIDGET_KEYS.has(e.code)) return false;
  // Стрелки внутри вкладок, деревьев, меню, списков, ползунков, тулбара и краёв панелей — их навигация.
  if (e.code.startsWith('Arrow') && e.target instanceof Element && e.target.closest(ARROW_ROLES)) return false;
  // Ctrl/Cmd+C, X, V идут событиями copy/cut/paste (системный буфер), а не в ядро.
  return !isClipboardKey(e);
}
