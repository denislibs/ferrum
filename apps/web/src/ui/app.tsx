// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).
// Настройки пользователя (тема, привязка к пикселям, раскладка клавиш, ширина
// панелей) читаются при запуске и сохраняются при каждом изменении.

import { createEffect, createMemo, createSignal, ErrorBoundary, on, onCleanup, onMount, Show } from 'solid-js';
import { boot } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { ConfirmDialog } from './dialog';
import { EditorContext, type Editor } from './context';
import { Menu, type MenuItem } from './menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { loadSettings, resolveTheme, saveSettings, storage, type Settings, type ThemeChoice } from './settings';
import { SettingsDialog } from './settings-dialog';
import { downloadBytes } from './files';
import { createUiStore } from './store';
import { Toasts } from './toasts';
import { Toolbar } from './toolbar';
import { TooltipLayer } from './tooltip';

export function App() {
  const store = createUiStore();
  const saved = loadSettings(storage());
  const [editor, setEditor] = createSignal<Editor>();
  const [failure, setFailure] = createSignal('');
  const [left, setLeft] = createSignal(saved.leftWidth);
  const [right, setRight] = createSignal(saved.rightWidth);
  const [pagesOpen, setPagesOpen] = createSignal(saved.pagesOpen);
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [context, setContext] = createSignal<{ x: number; y: number } | null>(null);
  // «Как в системе» следует за prefers-color-scheme на лету.
  const media = matchMedia('(prefers-color-scheme: dark)');
  const [prefersDark, setPrefersDark] = createSignal(media.matches);
  const onScheme = () => setPrefersDark(media.matches);
  media.addEventListener('change', onScheme);
  onCleanup(() => media.removeEventListener('change', onScheme));
  const theme = createMemo(() => resolveTheme(themeChoice(), prefersDark()));
  // Тема на <html> — до первого рисования, а не после: светлая не мигает тёмной.
  document.documentElement.dataset.theme = theme();
  let canvas!: HTMLCanvasElement;

  // Первая раскладка от ядра — умолчания: пока не пришла сохранённая, её не пишем
  // поверх (иначе неудачный импорт стёр бы настройки пользователя).
  let keymapSeen = saved.keymap ? 0 : 1;
  const [keymap, setKeymap] = createSignal(saved.keymap);
  createEffect(
    on(
      () => store.state.keymap.overrides,
      (o) => {
        if (!o) return;
        keymapSeen += 1;
        if (keymapSeen > 1) setKeymap(o);
      },
    ),
  );

  // Тема интерфейса и оверлеев ядра одна: смена доходит и до worker'а (цвет холста).
  createEffect(() => {
    document.documentElement.dataset.theme = theme();
    editor()?.send({ type: 'setTheme', theme: theme() });
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
  // Запись — не чаще раза в четверть секунды (края панелей тянутся каждый кадр), и
  // сразу при уходе со страницы.
  let pending: Settings | null = null;
  let writeTimer = 0;
  const flush = () => {
    clearTimeout(writeTimer);
    if (pending) saveSettings(storage(), pending);
    pending = null;
  };
  createEffect(() => {
    pending = {
      theme: themeChoice(),
      pixelSnap: pixelSnap(),
      keymap: keymap(),
      leftWidth: left(),
      rightWidth: right(),
      pagesOpen: pagesOpen(),
    };
    clearTimeout(writeTimer);
    writeTimer = window.setTimeout(flush, 250);
  });
  window.addEventListener('pagehide', flush);
  onCleanup(() => {
    window.removeEventListener('pagehide', flush);
    flush();
  });

  onMount(() => {
    boot(canvas, store, {
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme() });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        if (saved.keymap) {
          store.quietKeymapImport(() => setKeymap(''));
          ed.send({ type: 'importKeymap', json: saved.keymap });
        }
      },
    })
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
    { separator: true },
    { label: 'Настройки…', onSelect: () => setSettingsOpen(true) },
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
              <PropertiesPanel theme={theme()} />
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
            <Show when={settingsOpen()}>
              <SettingsDialog
                theme={themeChoice()}
                onTheme={setThemeChoice}
                pixelSnap={pixelSnap()}
                onPixelSnap={setPixelSnap}
                onClose={() => setSettingsOpen(false)}
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
