// Меню: выпадающие списки (файл, фигуры, пресеты, зум) и контекстное меню холста.
// role="menu" и клавиатура: стрелки, Home/End, Enter/Space, Esc (фокус
// возвращается туда, откуда меню открыли), Tab закрывает. Подпись сочетания у
// пункта — из действующей раскладки. Положение считается в фазе measure: меню
// сначала появляется невидимым, затем встаёт у якоря в пределах окна.

import { For, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useEditor } from './context';
import { Icon } from './icons';
import { shortcutLabel } from './shortcuts';

export type MenuItem =
  | {
      label: string;
      /** Ключ команды редактора: выполнить её и показать её сочетание. */
      command?: string;
      /** Своя подпись справа (размер пресета). */
      hint?: string;
      disabled?: boolean;
      checked?: boolean;
      onSelect?: () => void;
    }
  | { separator: true }
  | { heading: string };

export interface MenuProps {
  items: MenuItem[];
  /** Элемент, под которым открыть, или точка (контекстное меню). */
  anchor: HTMLElement | { x: number; y: number };
  label: string;
  /** Открыть над якорем (тулбар внизу экрана). */
  above?: boolean;
  onClose: () => void;
}

export function Menu(props: MenuProps): JSX.Element {
  const ed = useEditor();
  let el!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;

  const items = () => Array.from(el.querySelectorAll<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"]), [role="menuitemcheckbox"]:not([aria-disabled="true"])'));
  const close = (restore: boolean) => {
    props.onClose();
    if (restore) returnFocus?.focus?.();
  };

  const select = (item: Extract<MenuItem, { label: string }>) => {
    if (item.disabled) return;
    close(true);
    if (item.onSelect) item.onSelect();
    else if (item.command) ed.run(item.command);
  };

  const onKey = (e: KeyboardEvent) => {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    switch (e.key) {
      case 'ArrowDown':
        go(at + 1);
        break;
      case 'ArrowUp':
        go(at - 1);
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(list.length - 1);
        break;
      case 'Escape':
        close(true);
        break;
      case 'Tab':
        close(false);
        return;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  onMount(() => {
    ed.scheduler.measure(() => {
      const box = el.getBoundingClientRect();
      const a = props.anchor;
      const r = a instanceof HTMLElement ? a.getBoundingClientRect() : { left: a.x, right: a.x, top: a.y, bottom: a.y };
      let x = r.left;
      let y = props.above ? r.top - box.height - 4 : r.bottom + 4;
      if (x + box.width > innerWidth - 8) x = Math.max(8, innerWidth - 8 - box.width);
      if (y + box.height > innerHeight - 8) y = Math.max(8, r.top - box.height - 4);
      if (y < 8) y = 8;
      ed.scheduler.mutate(() => {
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        el.style.visibility = 'visible';
        items()[0]?.focus();
      });
    });
    const outside = (e: PointerEvent) => {
      if (!el.contains(e.target as Node)) close(false);
    };
    // Нажатие, которым меню открыли, ещё всплывает: слушаем со следующей задачи.
    const t = setTimeout(() => document.addEventListener('pointerdown', outside, true));
    window.addEventListener('blur', props.onClose);
    onCleanup(() => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('blur', props.onClose);
    });
  });

  const key = (command?: string) => {
    if (!command) return '';
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    return c?.bindings[0] ? shortcutLabel(c.bindings[0], ed.platform) : '';
  };

  return (
    <Portal>
      <div ref={el} class="fe-menu" role="menu" aria-label={props.label} style={{ visibility: 'hidden' }} onKeyDown={onKey}>
        <For each={props.items}>
          {(item) => {
            if ('separator' in item) return <div class="fe-menu__sep" role="separator" />;
            if ('heading' in item) return <div class="fe-menu__heading">{item.heading}</div>;
            return (
              <div
                class="fe-menu__item"
                role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                aria-checked={item.checked === undefined ? undefined : item.checked}
                aria-disabled={item.disabled ? 'true' : undefined}
                tabindex="-1"
                onClick={() => select(item)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    select(item);
                  }
                }}
              >
                <span class="fe-menu__check">
                  <Show when={item.checked}>
                    <Icon name="check" />
                  </Show>
                </span>
                <span class="fe-menu__label">{item.label}</span>
                <span class="fe-menu__key">{item.hint ?? key(item.command)}</span>
              </div>
            );
          }}
        </For>
      </div>
    </Portal>
  );
}
