// Диалог подтверждения (Dialog дизайн-системы): role="dialog", фокус на «Отмена»
// (безопасный выбор), Tab ходит по кнопкам, Esc отменяет, фокус возвращается туда,
// откуда диалог открыли.

import { onMount } from 'solid-js';
import { Portal } from 'solid-js/web';

export function ConfirmDialog(props: {
  title: string;
  text: string;
  confirm: string;
  cancel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  let box!: HTMLDivElement;
  let cancel!: HTMLButtonElement;
  const returnFocus = document.activeElement as HTMLElement | null;
  const close = (ok: boolean) => {
    returnFocus?.focus?.();
    if (ok) props.onConfirm();
    else props.onCancel();
  };
  onMount(() => cancel.focus());
  return (
    <Portal>
      <div class="fe-scrim" onPointerDown={(e) => e.target === e.currentTarget && close(false)}>
        <div
          ref={box}
          class="fe-dialog"
          role="dialog"
          aria-modal="true"
          // Щелчок по пустому месту диалога оставляет фокус в нём, а не роняет на body.
          tabindex="-1"
          aria-labelledby="fe-dialog-title"
          aria-describedby="fe-dialog-text"
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              close(false);
            } else if (e.key === 'Tab') {
              const b = Array.from(box.querySelectorAll<HTMLElement>('button'));
              const at = b.indexOf(document.activeElement as HTMLElement);
              e.preventDefault();
              b[(at + (e.shiftKey ? -1 : 1) + b.length) % b.length].focus();
            }
          }}
        >
          <h2 id="fe-dialog-title" class="fe-dialog__title">
            {props.title}
          </h2>
          <p id="fe-dialog-text" class="fe-dialog__text">
            {props.text}
          </p>
          <div class="fe-dialog__actions">
            <button ref={cancel} class="fe-btn" onClick={() => close(false)}>
              {props.cancel}
            </button>
            <button class="fe-btn fe-btn--primary" onClick={() => close(true)}>
              {props.confirm}
            </button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
