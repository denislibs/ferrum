// Уведомления: ошибки ядра и подтверждения действий («Экспортировано: …»), у правок с
// большими последствиями — кнопка «Отменить» (спека подсистемы 2, раздел 10). Каждое
// живёт 4 с и закрывается кнопкой; пока над ним курсор или в нём фокус, не исчезает.

import { For, Show } from 'solid-js';
import { Icon } from './icons';
import type { Toast, UiStore } from './store';

export function Toasts(props: { store: UiStore; onUndo?: (step: number) => void }) {
  /** Кнопки не забирают фокус у холста щелчком: уведомление уйдёт, а клавиши должны
   * по-прежнему доходить до него. С клавиатуры фокус возвращается на холст сам. */
  const keepFocus = (e: Event) => e.preventDefault();
  const close = (t: Toast, el: HTMLElement) => {
    const inside = el.contains(document.activeElement);
    props.store.dismissToast(t.id);
    if (inside) document.querySelector<HTMLElement>('canvas')?.focus();
  };
  return (
    <div class="fe-toasts" role="status" aria-live="polite">
      <For each={props.store.state.toasts}>
        {(t) => {
          let el!: HTMLDivElement;
          return (
            <div
              ref={el}
              class={`fe-toast fe-toast--${t.kind}`}
              onPointerEnter={() => props.store.pauseToast(t.id)}
              onPointerLeave={() => {
                if (!el.contains(document.activeElement)) props.store.startToast(t.id);
              }}
              onFocusIn={() => props.store.pauseToast(t.id)}
              onFocusOut={(e) => {
                if (!el.contains(e.relatedTarget as Node | null) && !el.matches(':hover')) props.store.startToast(t.id);
              }}
            >
              <Show when={t.kind === 'error'}>
                <Icon name="warning" />
                <span class="t-strong">Ошибка:</span>
              </Show>
              <span>{t.text}</span>
              <Show when={t.undoStep !== undefined && props.onUndo}>
                <button
                  class="fe-toast__action"
                  onPointerDown={keepFocus}
                  onMouseDown={keepFocus}
                  onClick={() => {
                    props.onUndo?.(t.undoStep!);
                    close(t, el);
                  }}
                >
                  Отменить
                </button>
              </Show>
              <button
                class="fe-toast__close"
                aria-label="Закрыть уведомление"
                onPointerDown={keepFocus}
                onMouseDown={keepFocus}
                onClick={() => close(t, el)}
              >
                <Icon name="close" size={12} />
              </button>
            </div>
          );
        }}
      </For>
    </div>
  );
}
