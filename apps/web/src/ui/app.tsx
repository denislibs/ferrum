// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).
// Настройки пользователя (тема, привязка к пикселям, раскладка клавиш, ширина
// панелей) читаются при запуске и сохраняются при каждом изменении. Документ —
// файл из хранилища (`file`): байты читает worker ввода-вывода, сохраняет `saver`;
// уход к экрану файлов или другому файлу сначала сохраняет текущий (`onLeave`).

import { createEffect, createMemo, createSignal, ErrorBoundary, onCleanup, onMount, Show } from 'solid-js';
import { detectPlatform } from '../core/core';
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
import { canvasMenu, DesignHeader, FileHeader, ResizeEdge } from './chrome';
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuItem } from './menu';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { diffSettings, resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
import { SettingsDialog } from './settings-dialog';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import type { Backend } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
import { createSaver, SETTLE_MS, withTimeout, type Saver, type SaverOptions } from './saver';
import { clipboardAction, eventShortcut } from './shortcuts';
import { installClipboard } from './system-clipboard';
import { createUiStore } from './store';
import { Toasts } from './toasts';
import { Toolbar } from './toolbar';
import { TooltipLayer } from './tooltip';

export interface AppProps {
  file: FileMeta;
  files: FileStore;
  /** Настройки пользователя (IndexedDB, общие для вкладок). */
  settings: SettingsStore;
  /** Уход с редактора: сохранить и остановить ядро; `false` — запись не удалась, остаться. */
  onLeave: (leave: () => Promise<boolean>) => void;
  /** Файл из хранилища не открылся (испорчен): к экрану файлов с этим сообщением. */
  onOpenFailed: (message: string) => void;
  /** Редактор запущен заново после сбоя ядра или потери GPU: что сказать пользователю. */
  restart?: Restart;
  /** Бэкенд рендера (после потери GPU — запасной). */
  backend?: Backend;
  /** Ядро остановилось (или GPU недоступен): открыть этот же файл в новом редакторе.
   * `false` — перезапусков слишком много, остаётся плашка остановки. */
  onRestart: (r: Restart) => boolean;
  navigate: (r: Route) => void;
}

/** Тестовый режим: интервалы журнала и полного файла из адреса (`?journal=`, `?fullSave=`). */
function testIntervals(): SaverOptions {
  if (!isTestMode()) return {};
  const p = new URLSearchParams(location.search);
  const ms = (k: string) => {
    const v = Number(p.get(k));
    return p.has(k) && Number.isFinite(v) && v > 0 ? v : undefined;
  };
  return { journalMs: ms('journal'), fullIdleMs: ms('fullSave') };
}

/** Последняя запись перед остановкой рендера (потеря GPU без перезапуска): снимок
 * и полный файл — не дольше этого, дальше ядро останавливается без них. */
const SAVE_STOP_MS = SETTLE_MS * 2;

export function App(props: AppProps) {
  const store = createUiStore();
  const saved = props.settings.current;
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
  // Экран запуска уходит, когда ядро готово или запуск не удался (тогда видна ошибка).
  createEffect(() => {
    if (failure() || store.state.stopped) dismissSplash(true);
    else if (store.state.ready) dismissSplash();
  });

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
  // Что этот редактор уже записал (или принял от другой вкладки): пишутся только поля,
  // изменённые здесь, — чужие (вид списка, раскладка панелей другой вкладки) не
  // затираются своими прежними значениями.
  let written: Settings = { ...saved };
  const flush = () => {
    clearTimeout(writeTimer);
    if (pending) {
      const changed = diffSettings(written, pending);
      written = pending;
      if (Object.keys(changed).length > 0) props.settings.update(changed);
    }
    pending = null;
  };
  // Настройки, изменённые в другой вкладке: тема, привязка и раскладка — на лету
  // (ширина панелей у каждой вкладки своя).
  const offRemote = props.settings.onChange((p) => {
    // Принятое от другой вкладки — уже в базе: обратно не пишется.
    if (p.theme) {
      written = { ...written, theme: p.theme };
      setThemeChoice(p.theme);
    }
    if (p.pixelSnap !== undefined) {
      written = { ...written, pixelSnap: p.pixelSnap };
      setPixelSnap(p.pixelSnap);
    }
    if (p.keymap !== undefined && p.keymap !== keymap()) {
      const prev = keymap();
      const next = p.keymap;
      written = { ...written, keymap: next };
      setKeymap(next);
      const ed = editor();
      if (ed) {
        // Ядро не приняло чужую раскладку — назад к прежней, без записи.
        store.quietKeymapImport(undefined, () => {
          written = { ...written, keymap: prev };
          setKeymap(prev);
          props.settings.rollback({ keymap: prev });
        });
        ed.send(next ? { type: 'importKeymap', json: next } : { type: 'resetKeymap', command: undefined });
      }
      // Ядра ещё нет — импортирует `beforeDocument` (нынешнюю, а не прочитанную при запуске).
    }
  });
  const offErrors = props.settings.onError((m) => store.toast(m, 'error'));
  onCleanup(() => {
    offRemote();
    offErrors();
  });
  createEffect(() => {
    pending = {
      ...written,
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

  const id = props.file.id;
  let saver: Saver | undefined;
  let offSaveKey: (() => void) | undefined;
  let disposed = false;
  // Байты файла читаются, пока запускается ядро. Не прочитались — к экрану файлов.
  let stored = false;
  /** Поколение файла при открытии: с него начинается проверка записи. */
  let version = props.file.version;
  /** Наибольший сеанс пачек журнала файла: сеанс этого редактора — после него. */
  let journalSession: number | null = null;
  const bytes = props.files.read(id).then(
    (r) => {
      stored = r.bytes !== null;
      version = r.meta.version;
      journalSession = r.session;
      if (!r.bytes) return null;
      let journal = r.journal;
      const total = journal.reduce((n, b) => n + b.byteLength, r.bytes.byteLength);
      if (r.journalTooLarge || journal.length > MAX_JOURNAL_ENTRIES || total > MAX_OPEN_BYTES) {
        // Журнал сверх пределов (пачек больше 100 000 или файл с журналом больше 512 МБ;
        // хранилище могло не читать его вовсе): ядро отвергло бы всё открытие — открыть
        // хотя бы файл. Первая полная запись сотрёт прежние сеансы журнала.
        store.toast('Журнал изменений слишком большой — файл открыт без несохранённых правок', 'error');
        journal = [];
      }
      return { bytes: r.bytes, journal };
    },
    (e: unknown) => {
      props.onOpenFailed(`Не удалось открыть файл: ${e instanceof Error ? e.message : String(e)}`);
      return null;
    },
  );
  // Сохранить не удалось (ядро остановлено, снимок или запись не прошли): уйти
  // можно, но только с вопросом — причина в тексте, «Остаться» — по умолчанию.
  const [askLeave, setAskLeave] = createSignal<{ reason: string; answer: (go: boolean) => void } | null>(null);
  props.onLeave(async () => {
    if (!saver) return true;
    // Ядро остановлено: запись либо сразу отвечает (сохранять нечего или нельзя), либо
    // повисла ещё до остановки — тогда не ждать её вечно, а спросить.
    const saved = store.state.stopped ? withTimeout(saver.save(), SETTLE_MS, false) : saver.save();
    if (await saved) return true;
    // Редактор уже снят (перезапуск после сбоя ядра во время ухода): спросить
    // некому — остаться, иначе уход корня ждал бы ответа вечно.
    if (disposed) return false;
    const reason = saver.failure() ?? 'Изменения не записаны в хранилище';
    return new Promise<boolean>((answer) => setAskLeave({ reason, answer }));
  });
  const answerLeave = (go: boolean) => {
    const a = askLeave();
    setAskLeave(null);
    a?.answer(go);
  };

  onMount(() => {
    boot(canvas, store, {
      document: bytes,
      backend: props.backend,
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme() });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        // Нынешняя раскладка: другая вкладка могла сменить её, пока ядро запускалось.
        const km = keymap();
        if (km) {
          setRestored(false);
          store.quietKeymapImport(
            () => setRestored(true),
            () => {
              setKeymap('');
              setRestored(true);
            },
          );
          ed.send({ type: 'importKeymap', json: km });
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
        if (disposed) {
          ed.dispose();
          return;
        }
        setEditor(ed);
        canvas.focus();
        uninstall = installClipboard(ed);
        saver = createSaver(ed, props.files, id, version, stored, { ...testIntervals(), journalSession });
        // Ctrl/Cmd+S, пока фокус в поле панели (клавиша не ушла ядру): тоже
        // «Сохранить», и браузер не открывает диалог сохранения страницы.
        const onSaveKey = (e: KeyboardEvent) => {
          const s = eventShortcut(e);
          if (e.defaultPrevented || !s) return;
          if (!store.state.keymap.commands.find((c) => c.key === 'file.save')?.bindings.includes(s)) return;
          e.preventDefault();
          void saver?.save();
        };
        window.addEventListener('keydown', onSaveKey);
        offSaveKey = () => window.removeEventListener('keydown', onSaveKey);
        // Открытие файла из хранилища: отказ самого открытия (а не, скажем, раскладки)
        // до `opened` — файл испорчен; запись остаётся, пользователь — на экране файлов.
        let opening = true;
        let recovered = false;
        ed.core.on((e) => {
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
            opening = false;
            if (props.restart) {
              // После потери GPU — рендер, который новое ядро действительно получило:
              // принудительный бэкенд, не запустившись, уступает CPU.
              const r = props.restart.reason === 'gpu' ? { ...props.restart, backend: ed.core.backend ?? props.restart.backend } : props.restart;
              store.toast(restartText(r), r.lost ? 'error' : 'info');
            }
            else if (recovered) store.toast('Восстановлены изменения, не записанные в файл');
            void props.files.touch(id).catch(() => {});
            // Имя файла — из хранилища (его могли переименовать на экране файлов);
            // тихо: открытие — не правка, файл из-за него не пересохраняется.
            if (e.name !== props.file.name) ed.send({ type: 'setDocumentName', name: props.file.name, quiet: true });
          } else if (opening && stored && e.type === 'error' && e.message.startsWith('Не удалось открыть файл')) {
            opening = false;
            props.onOpenFailed(e.message);
          } else if (e.type === 'documentName') {
            // Переименовали в редакторе — запись в хранилище тоже (без ожидания снимка).
            props.files.rename(id, e.name).catch((err: unknown) => store.toast(`Не удалось переименовать файл: ${err instanceof Error ? err.message : String(err)}`, 'error'));
          }
          if (!opening && e.type === 'error' && e.message.startsWith('Ядро остановлено:')) {
            // Сбой ядра после открытия (паника, исключение в worker'е): тот же файл — в
            // новом редакторе, правки — из файла и журнала. Сбой во время открытия
            // перезапуска не вызывает: испорченный журнал крутил бы его вечно.
            console.error(e.message);
            // Потеряно ли что-то — когда пачки, отданные ядром до сбоя, дописаны (или нет).
            const s = saver;
            // Пока ждали пачки, редактор могли снять (ушли к другому файлу): перезапуск
            // старого открыл бы его поверх нового.
            const restart = (lost: boolean) => {
              if (!disposed) props.onRestart({ reason: 'crash', lost, message: e.message });
            };
            // Хранилище не ответило — исход пачек неизвестен: честнее «не сохранились».
            if (s) void s.settled().then((ok) => restart(!ok || s.pending()));
            else restart(false);
          }
          // Потеря поверхности GPU: worker создаёт её заново; причина — в консоль, даже
          // если пересоздать удастся.
          if (e.type === 'surfaceLost') console.warn(e.message);
          if (e.type === 'surfaceRestored') store.toast('Видеокарта перезапущена');
          if (e.type === 'surfaceFailed') {
            // Поверхность GPU не пережила потерю: правки — в журнал (ядро живо), и тот же
            // файл — в новом редакторе с запасным бэкендом (контекст холста — один раз).
            // Перезапуск — тот же, что после сбоя ядра (блокировка файла, предел два раза
            // в минуту на оба вида).
            const { next, message } = e;
            console.error(message);
            const s = saver;
            // Хранилище не ответило за `SETTLE_MS` — исход пачки неизвестен: «не сохранились».
            const flushed = s ? withTimeout(s.flushJournal(), SETTLE_MS, false) : Promise.resolve(true);
            void flushed.then(async (ok) => {
              // Пока писали журнал, редактор могли снять: перезапуск старого открыл бы его
              // поверх нового.
              if (disposed) return;
              if (props.onRestart({ reason: 'gpu', lost: !ok || (s?.pending() ?? false), backend: next, message })) return;
              // Перезапускать больше нельзя: холст мёртв навсегда, а правки вслепую хуже
              // остановки. Пока ядро живо — полный файл (уход потом без вопроса), затем
              // ядро остановлено: кадров и ввода нет, плашка — как при сбое ядра.
              // Запись не ответила за `SAVE_STOP_MS` (снимок или хранилище повисли) — стоп
              // всё равно: признак изменений остаётся, уход спросит о несохранённом.
              if (s && (await withTimeout(s.save(), SAVE_STOP_MS, false)) && store.state.modified) {
                // Записано: ядро ответит «изменений нет» (`markSaved`) — его ждёт сохранитель
                // (уход без вопроса). Не ответило за `SETTLE_MS` — стоп всё равно. Событие
                // в состояние интерфейса кладёт кадр планировщика, а кадров после остановки
                // нет — признак снимается здесь.
                const clean = await withTimeout(
                  new Promise<boolean>((resolve) => {
                    const off = ed.core.on((m) => {
                      if (m.type === 'modified' && !m.modified) {
                        off();
                        resolve(true);
                      }
                    });
                  }),
                  SETTLE_MS,
                  false,
                );
                if (clean) store.setState('modified', false);
              }
              if (disposed) return;
              ed.core.dispose();
              store.setState('stopped', (prev) => prev ?? `Рендер остановлен: ${message}`);
            });
          }
          if (e.type === 'exported') {
            downloadBytes(e.name, e.bytes, 'image/png');
            store.toast(`Экспортировано: ${e.name}`);
          } else if (e.type === 'saved') {
            // «Сохранить копию на диск…»: байты готовит worker и передаёт без копии.
            downloadBytes(fileName(store.state.documentName, '.ferrum'), e.bytes, 'application/octet-stream');
          }
        });
      })
      // Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
      .catch((e: unknown) => setFailure(e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`));
  });
  onCleanup(() => {
    disposed = true;
    // Открытый вопрос об уходе (редактор снят перезапуском) — «Остаться».
    answerLeave(false);
    offSaveKey?.();
    uninstall?.();
    saver?.dispose();
    editor()?.dispose();
  });

  // «Новый файл» и «Открыть…» не спрашивают о несохранённом: смена маршрута сначала
  // сохраняет текущий файл в хранилище, затем открывает другой.
  const openFromDisk = async () => {
    const meta = await importFromDisk(props.files, (t) => store.toast(t, 'error'));
    if (meta) props.navigate({ kind: 'file', id: meta.id });
  };
  // File System Access: файл на диске, связанный с записью (правки пишутся в него).
  const openLinked = async () => {
    try {
      const meta = await props.files.disk?.open();
      if (meta) props.navigate({ kind: 'file', id: meta.id });
    } catch (e) {
      store.toast(`Не удалось открыть файл: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  const saveToDisk = async () => {
    try {
      // Сначала — последнее в хранилище: на диск уходит свежий файл.
      if (saver && !(await saver.save())) return;
      const meta = await props.files.disk?.saveAs(id, fileName(store.state.documentName, '.ferrum'));
      if (meta) store.toast(`Файл сохранён на диск: ${meta.disk}`);
    } catch (e) {
      store.toast(`Не удалось сохранить на диск: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  // «Сохранить копию на диск…»: в десктопе — родной диалог (копия сохранённого файла),
  // в браузере — байты из ядра скачиванием.
  const saveCopy = async (ed: Editor) => {
    const copy = props.files.disk?.saveCopy;
    if (!copy) {
      ed.send({ type: 'save' });
      return;
    }
    try {
      if (saver && !(await saver.save())) return;
      if (await copy(id, fileName(store.state.documentName, '.ferrum'))) store.toast('Копия сохранена');
    } catch (e) {
      store.toast(`Не удалось сохранить копию: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  const disk = (items: MenuItem[]): MenuItem[] => (props.files.disk ? items : []);
  const fileItems = (ed: Editor): MenuItem[] => [
    { label: 'Новый файл', onSelect: () => props.navigate({ kind: 'new' }) },
    { label: 'Открыть…', onSelect: () => void openFromDisk() },
    ...disk([{ label: 'Открыть с диска…', onSelect: () => void openLinked() }]),
    { label: 'Сохранить', command: 'file.save', onSelect: () => void saver?.save() },
    ...disk([{ label: 'Сохранить на диск…', onSelect: () => void saveToDisk() }]),
    { label: 'Сохранить копию на диск…', onSelect: () => void saveCopy(ed) },
    { separator: true },
    { label: 'Все файлы', onSelect: () => props.navigate({ kind: 'files', section: 'recent' }) },
    { label: 'Настройки…', onSelect: () => setSettingsOpen(true) },
  ];

  return (
    <ErrorBoundary
      fallback={(err: unknown) => (dismissSplash(true),
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
        <Show when={!store.state.ready && failure()}>
          <div class="fe-boot fe-boot--error" role="alert">
            {failure()}
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
              <FileHeader items={() => fileItems(ed())} onBack={() => props.navigate({ kind: 'files', section: 'recent' })} />
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
      <Show when={askLeave()}>
        {(a) => (
          <ConfirmDialog
            title="Изменения не сохранены — уйти?"
            text={a().reason}
            confirm="Уйти"
            cancel="Остаться"
            onCancel={() => answerLeave(false)}
            onConfirm={() => answerLeave(true)}
          />
        )}
      </Show>
      <Toasts store={store} />
    </div>
    </ErrorBoundary>
  );
}
