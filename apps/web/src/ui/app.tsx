// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).
// Настройки пользователя (тема, привязка к пикселям, раскладка клавиш, ширина
// панелей) читаются при запуске и сохраняются при каждом изменении.

import { createEffect, createMemo, createSignal, ErrorBoundary, onCleanup, onMount, Show } from 'solid-js';
import { detectPlatform } from '../core/core';
import { MAX_COMMAND_BYTES } from '../core/messages';
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
import { downloadBytes, FERRUM_FILE, fileBytes, fileName, pickFile } from './files';
import { clipboardAction, eventShortcut } from './shortcuts';
import { installClipboard } from './system-clipboard';
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
  const platform = detectPlatform();
  let uninstall: (() => void) | undefined;
  let canvas!: HTMLCanvasElement;

  // Раскладка в хранилище — отличия, как их отдаёт ядро. Пока сохранённая не
  // принята (или её нет), умолчания ядра поверх неё не пишем: иначе неудачный
  // импорт стёр бы настройки пользователя. Принята — пишем очищенный набор ядра
  // (пропущенное в нём уже отброшено, предупреждение не повторится при запуске).
  const [keymap, setKeymap] = createSignal(saved.keymap);
  const [restored, setRestored] = createSignal(!saved.keymap);
  createEffect(() => {
    const o = store.state.keymap.overrides;
    if (restored() && o && store.state.keymap.commands.length > 0) setKeymap(o);
  });

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
          store.quietKeymapImport(
            () => setRestored(true),
            () => {
              setKeymap('');
              setRestored(true);
            },
          );
          ed.send({ type: 'importKeymap', json: saved.keymap });
        }
      },
      // Ctrl/Cmd+C, X, V, пока они вызывают копирование, вырезание и вставку, идут
      // событиями буфера обмена (системный буфер), а не клавишами в ядро.
      isClipboardKey: (e) => {
        const s = eventShortcut(e);
        const action = s && clipboardAction(s, platform);
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste' }[action];
        return !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
      },
    })
      .then((ed) => {
        setEditor(ed);
        canvas.focus();
        uninstall = installClipboard(ed);
        ed.core.on((e) => {
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          } else if (e.type === 'saved') {
            // Байты готовит worker и передаёт без копии; здесь — только скачивание.
            downloadBytes(fileName(store.state.documentName, '.ferrum'), e.bytes, 'application/octet-stream');
          }
        });
      })
      // Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`));
  });
  onCleanup(() => {
    uninstall?.();
    editor()?.core.dispose();
  });

  // Выбор файла и байты — worker'у; разбор (и отказ испорченного файла одной
  // ошибкой с текущим документом на месте) — в ядре.
  const openFile = async (ed: Editor) => {
    try {
      const f = await pickFile(FERRUM_FILE);
      if (!f) return;
      // Размер — до чтения: файл больше предела не копируется в память ради отказа.
      if (f.size > MAX_COMMAND_BYTES.open) {
        store.toast(`Не удалось открыть файл: больше ${MAX_COMMAND_BYTES.open / (1024 * 1024)} МБ`, 'error');
        return;
      }
      ed.send({ type: 'open', bytes: await fileBytes(f) });
    } catch (e) {
      store.toast(`Не удалось открыть файл: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };

  // «Создать» и «Открыть…» заменяют документ: пока автосохранения нет, при правках
  // с открытия, создания или сохранения (и вне истории отмены: страницы, фон, имя
  // файла) — с вопросом; «Отмена» оставляет текущий документ.
  const REPLACE = {
    new: { title: 'Создать новый файл?', confirm: 'Создать', run: (ed: Editor) => ed.send({ type: 'demo', nodes: 0 }) },
    open: { title: 'Открыть другой файл?', confirm: 'Открыть', run: (ed: Editor) => void openFile(ed) },
  } as const;
  const [confirmReplace, setConfirmReplace] = createSignal<keyof typeof REPLACE | null>(null);
  const replaceDocument = (ed: Editor, kind: keyof typeof REPLACE) => {
    if (store.state.modified) setConfirmReplace(kind);
    else REPLACE[kind].run(ed);
  };
  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Создать', onSelect: () => replaceDocument(ed, 'new') },
    { label: 'Открыть…', onSelect: () => replaceDocument(ed, 'open') },
    { label: 'Сохранить как…', onSelect: () => ed.send({ type: 'save' }) },
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
            <Show when={confirmReplace()}>
              {(kind) => (
                <ConfirmDialog
                  title={REPLACE[kind()].title}
                  text="Несохранённые изменения будут потеряны."
                  confirm={REPLACE[kind()].confirm}
                  cancel="Отмена"
                  onCancel={() => setConfirmReplace(null)}
                  onConfirm={() => {
                    const run = REPLACE[kind()].run;
                    setConfirmReplace(null);
                    run(ed());
                  }}
                />
              )}
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
