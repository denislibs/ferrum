// «Нет доступа» (спека подсистемы 4, раздел 5): файл по ссылке без права на него. Несуществующий и чужой
// файл неотличимы, поэтому просьба о доступе есть в обоих случаях, а ответ на неё один.

import { createSignal, Show } from 'solid-js';
import type { FileStore } from './file-store';
import type { Route } from './router';

export function NoAccessScreen(props: { id: string; files: FileStore; navigate: (r: Route) => void }) {
  const [state, setState] = createSignal<'idle' | 'busy' | 'sent' | 'failed'>('idle');
  const request = async () => {
    setState('busy');
    const ok = await (props.files.requestAccess?.(props.id) ?? Promise.resolve(false)).catch(() => false);
    setState(ok ? 'sent' : 'failed');
  };
  return (
    <div class="fe-denied" role="main">
      <div class="fe-denied__card">
        <h1 class="fe-denied__title t-heading">Нет доступа</h1>
        <p class="fe-denied__text t-muted">У вас нет доступа к этому файлу, или его больше нет.</p>
        <Show when={state() === 'sent'}>
          <p class="fe-denied__text" role="status">
            Запрос отправлен владельцу файла
          </p>
        </Show>
        <Show when={state() === 'failed'}>
          <p class="fe-denied__text" role="alert">
            Не удалось отправить запрос. Попробуйте ещё раз
          </p>
        </Show>
        <div class="fe-denied__actions">
          <button class="fe-btn fe-btn--secondary" onClick={() => props.navigate({ kind: 'files', section: 'recent' })}>
            К файлам
          </button>
          <Show when={state() !== 'sent'}>
            <button class="fe-btn fe-btn--primary" disabled={state() === 'busy'} onClick={() => void request()}>
              Запросить доступ
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
