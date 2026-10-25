// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).

import { createSignal, ErrorBoundary, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { ConfirmDialog } from './dialog';
import { EditorContext, type Editor } from './context';
import { Menu, type MenuItem } from './menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { downloadBytes } from './files';
import { createUiStore } from './store';
import { Toasts } from './toasts';
import { Toolbar } from './toolbar';
import { TooltipLayer } from './tooltip';

export function App() {
  const store = createUiStore();
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  const [left, setLeft] = createSignal(248);
  const [right, setRight] = createSignal(248);
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  const [pagesOpen, setPagesOpen] = createSignal(true);
  let canvas!: HTMLCanvasElement;

  onMount(() => {
    boot(canvas, store)
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
        ed.core.on((e) => {
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          }
        });
      })
      // Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`));
  });
  onCleanup(() => editor()?.core.dispose());

  // «Создать» заменяет документ: пока автосохранения нет, при правках — с вопросом.
  const [confirmNew, setConfirmNew] = createSignal(false);
  const fileItems = (ed: Editor): MenuItem[] => [
    {
      label: 'Создать',
      onSelect: () => {
        const s = store.state;
        if (s.canUndo || s.canRedo) setConfirmNew(true);
        else ed.send({ type: 'demo', nodes: 0 });
      },
    },
  ];

  return (
    <ErrorBoundary
      fallback={(err: unknown) => (
        <div class="fe-boot fe-boot--error" role="alert">
          Ошибка интерфейса: {err instanceof Error ? err.message : String(err)}. Перезагрузите страницу
        </div>
      )}
    >
    <div
      class="fe-app"
      classList={{ 'fe-app--bare': store.state.uiHidden }}
      style={{ '--left-w': `${left()}px`, '--right-w': `${right()}px` }}
    >
      <main class="fe-stage">
        <canvas
          ref={canvas}
          class="fe-canvas"
          tabindex="0"
          aria-label="Холст"
          onContextMenu={(e) => {
            if (editor()) setContext({ x: e.clientX, y: e.clientY });
          }}
        />
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
        <Show when={editor()}>
          {(ed) => (
            <EditorContext.Provider value={ed()}>
              <Toolbar />
            </EditorContext.Provider>
          )}
        </Show>
      </main>
      <Show when={editor()}>
        {(ed) => (
          <EditorContext.Provider value={ed()}>
            <aside class="fe-side fe-side--left" aria-label="Страницы и слои">
              <FileHeader items={() => fileItems(ed())} />
              <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} />
              <LayersPanel />
              <ResizeEdge side="left" width={left()} onWidth={setLeft} />
            </aside>
            <aside class="fe-side fe-side--right" aria-label="Свойства">
              <DesignHeader />
              <PropertiesPanel />
              <ResizeEdge side="right" width={right()} onWidth={setRight} />
            </aside>
            <TooltipLayer />
            <Show when={confirmNew()}>
              <ConfirmDialog
                title="Создать новый файл?"
                text="Несохранённые изменения будут потеряны."
                confirm="Создать"
                cancel="Отмена"
                onCancel={() => setConfirmNew(false)}
                onConfirm={() => {
                  setConfirmNew(false);
                  ed().send({ type: 'demo', nodes: 0 });
                }}
              />
            </Show>
            <Show when={context()}>
              {(at) => (
                <Menu
                  label="Действия с выделением"
                  items={canvasMenu(store.state.selectionCount)}
                  anchor={at()}
                  onClose={() => setContext(null)}
                />
              )}
            </Show>
          </EditorContext.Provider>
        )}
      </Show>
      <Toasts store={store} />
    </div>
    </ErrorBoundary>
  );
}
