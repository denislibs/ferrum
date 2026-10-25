// Меню: выпадающие списки (файл, фигуры, пресеты, зум) и контекстное меню холста.
// role="menu" и клавиатура: стрелки, Home/End, Enter/Space, Esc (фокус
// возвращается туда, откуда меню открыли), Tab закрывает. Подпись сочетания у
// пункта — из действующей раскладки. Положение считается в фазе measure: меню
// сначала появляется невидимым, затем встаёт у якоря в пределах окна.

import { Index, Match, onCleanup, onMount, Show, Switch, type JSX } from 'solid-js';
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
    return c?.bindings[0] ? shortcutLabel(c.bindings[0], ed.platform, ed.layout()) : '';
  };

  return (
    <Portal>
      <div ref={el} class="fe-menu" role="menu" aria-label={props.label} style={{ visibility: 'hidden' }} onKeyDown={onKey}>
        {/* Index, не For: при перестройке пунктов (зум изменился) узлы остаются, фокус не теряется. */}
        <Index each={props.items}>
          {(item) => (
            <Switch>
              <Match when={'separator' in item() && item()}>
                <div class="fe-menu__sep" role="separator" />
              </Match>
              <Match when={'heading' in item() && (item() as { heading: string })}>
                {(h) => <div class="fe-menu__heading">{h().heading}</div>}
              </Match>
              <Match when={'label' in item() && (item() as Extract<MenuItem, { label: string }>)}>
                {(it) => (
                  <div
                    class="fe-menu__item"
                    role={it().checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                    aria-checked={it().checked === undefined ? undefined : it().checked}
                    aria-disabled={it().disabled ? 'true' : undefined}
                    tabindex="-1"
                    onClick={() => select(it())}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        select(it());
                      }
                    }}
                  >
                    <span class="fe-menu__check">
                      <Show when={it().checked}>
                        <Icon name="check" />
                      </Show>
                    </span>
                    <span class="fe-menu__label">{it().label}</span>
                    <span class="fe-menu__key">{it().hint ?? key(it().command)}</span>
                  </div>
                )}
              </Match>
            </Switch>
          )}
        </Index>
      </div>
    </Portal>
  );
}
