// Уведомления: ошибки ядра и подтверждения действий («Экспортировано: …»), у правок с
// большими последствиями — кнопка «Отменить» (спека подсистемы 2, раздел 10). Каждое
// живёт 4 с и закрывается кнопкой; пока над ним курсор или в нём фокус, не исчезает.

import { createEffect, For, on, Show } from 'solid-js';
import { Icon } from './icons';
import type { Toast, UiStore } from './store';

export function Toasts(props: { store: UiStore; onUndo?: (step: number) => void }) {
  /** Кнопки не забирают фокус у холста щелчком: уведомление уйдёт, а клавиши должны
   * по-прежнему доходить до него. С клавиатуры фокус возвращается на холст сам. */
  const keepFocus = (e: Event) => e.preventDefault();
  const canvas = () => document.querySelector<HTMLElement>('canvas');
  /** Закрытие: фокус из уведомления уходит на холст. Элемент остаётся на 400 мс
   * невидимым и перехватывает указатель — вторая половина двойного щелчка не должна
   * попасть в холст под ним и сменить только что восстановленное выделение. */
  const close = (t: Toast, el: HTMLElement) => {
    if (el.classList.contains('fe-toast--gone')) return;
    const inside = el.contains(document.activeElement);
    props.store.pauseToast(t.id);
    el.classList.add('fe-toast--gone');
    setTimeout(() => props.store.dismissToast(t.id), 400);
    if (inside) canvas()?.focus();
  };
  // Фокус был в уведомлении, а его заменили или вытеснили: он не должен упасть на body.
  let root!: HTMLDivElement;
  let hadFocus = false;
  createEffect(
    on(
      () => props.store.state.toasts.map((t) => t.id).join(),
      () =>
        queueMicrotask(() => {
          const a = document.activeElement;
          if (hadFocus && (!a || a === document.body) && !root.contains(a)) canvas()?.focus();
          hadFocus = false;
        }),
      { defer: true },
    ),
  );
  return (
    <div
      ref={root}
      class="fe-toasts"
      role="status"
      aria-live="polite"
      aria-atomic="false"
      onFocusIn={() => (hadFocus = true)}
      onFocusOut={(e) => {
        if (e.relatedTarget) hadFocus = root.contains(e.relatedTarget as Node);
      }}
    >
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
              onKeyDown={(e) => {
                if (e.key !== 'Escape') return;
                e.preventDefault();
                close(t, el);
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
