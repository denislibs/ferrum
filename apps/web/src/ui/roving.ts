// Клавиатура составных виджетов (toolbar, tablist): одна точка Tab на весь виджет
// («roving tabindex»), Left/Right/Home/End переводят фокус между элементами —
// отключённые (`aria-disabled`) тоже, чтобы у них была видна подсказка.

/** Включает навигацию на контейнере; возвращает отписку. */
export function roving(root: HTMLElement, itemSelector: string): () => void {
  const items = () => Array.from(root.querySelectorAll<HTMLElement>(itemSelector));
  const setStop = (target: HTMLElement | undefined) => {
    const all = items();
    const stop = target && all.includes(target) ? target : all[0];
    for (const it of all) it.tabIndex = it === stop ? 0 : -1;
  };
  setStop(undefined);
  const focusin = (e: FocusEvent) => {
    if (e.target instanceof HTMLElement && items().includes(e.target)) setStop(e.target);
  };
  const keydown = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
    const all = items();
    const at = all.indexOf(e.target as HTMLElement);
    if (at < 0) return;
    let next: number;
    switch (e.key) {
      case 'ArrowRight':
        next = (at + 1) % all.length;
        break;
      case 'ArrowLeft':
        next = (at - 1 + all.length) % all.length;
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = all.length - 1;
        break;
      default:
        return;
    }
    e.preventDefault();
    all[next].focus();
  };
  root.addEventListener('focusin', focusin);
  root.addEventListener('keydown', keydown);
  return () => {
    root.removeEventListener('focusin', focusin);
    root.removeEventListener('keydown', keydown);
  };
}
