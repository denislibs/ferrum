// Редактор: холст под управлением worker'а и панели вокруг него.

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { EditorContext, type Editor } from './context';
import { createUiStore } from './store';
import { Toasts } from './toasts';

export function App() {
  const store = createUiStore();
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  let canvas!: HTMLCanvasElement;

  onMount(() => {
    boot(canvas, store)
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
      })
      // Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`));
  });
  onCleanup(() => editor()?.core.dispose());

  return (
    <div class="fe-app" classList={{ 'fe-app--bare': store.state.uiHidden }}>
      <main class="fe-stage">
        <canvas ref={canvas} class="fe-canvas" tabindex="0" aria-label="Холст" />
        <Show when={!store.state.ready}>
          <div class="fe-boot" classList={{ 'fe-boot--error': !!failure() }} role="status">
            {failure() || 'Запуск редактора…'}
          </div>
        </Show>
        <Show when={store.state.stopped}>
          {(message) => (
            <div class="fe-boot fe-boot--error fe-boot--stopped" role="alert">
              {message()}. Правки больше не принимаются — перезагрузите страницу
            </div>
          )}
        </Show>
      </main>
      <Show when={editor()}>{(ed) => <EditorContext.Provider value={ed()}>{null}</EditorContext.Provider>}</Show>
      <Toasts store={store} />
    </div>
  );
}
