// Уведомления: ошибки ядра и подтверждения действий («Экспортировано: …»).

import { For, Show } from 'solid-js';
import { Icon } from './icons';
import type { UiStore } from './store';

export function Toasts(props: { store: UiStore }) {
  return (
    <div class="fe-toasts" role="status" aria-live="polite">
      <For each={props.store.state.toasts}>
        {(t) => (
          <div class={`fe-toast fe-toast--${t.kind}`}>
            <Show when={t.kind === 'error'}>
              <Icon name="warning" />
              <span class="t-strong">Ошибка:</span>
            </Show>
            <span>{t.text}</span>
          </div>
        )}
      </For>
    </div>
  );
}
