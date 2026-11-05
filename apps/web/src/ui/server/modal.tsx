// Диалог с полями ввода (стиль Dialog дизайн-системы): Tab ходит внутри, Esc закрывает, фокус
// возвращается туда, откуда диалог открыли.

import { onCleanup, onMount, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';

const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex="0"]';

export function Modal(props: { title: string; onClose: () => void; children: JSX.Element }) {
  let box!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;
  onMount(() => box.querySelector<HTMLElement>('input, select, button')?.focus());
  onCleanup(() => returnFocus?.focus?.());
  return (
    <Portal>
      <div class="fe-scrim" onPointerDown={(e) => e.target === e.currentTarget && props.onClose()}>
        <div
          ref={box}
          class="fe-dialog fe-modal"
          role="dialog"
          aria-modal="true"
          tabindex="-1"
          aria-label={props.title}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              props.onClose();
            } else if (e.key === 'Tab') {
              const items = Array.from(box.querySelectorAll<HTMLElement>(FOCUSABLE));
              if (items.length === 0) return;
              const at = items.indexOf(document.activeElement as HTMLElement);
              e.preventDefault();
              items[(at + (e.shiftKey ? -1 : 1) + items.length) % items.length].focus();
            }
          }}
        >
          <h2 class="fe-dialog__title">{props.title}</h2>
          {props.children}
        </div>
      </div>
    </Portal>
  );
}
