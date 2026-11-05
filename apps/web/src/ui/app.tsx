// Редактор: холст под управлением worker'а и панели вокруг него (раскладка UI3).
// Настройки пользователя (тема, привязка к пикселям, раскладка клавиш, ширина
// панелей) читаются при запуске и сохраняются при каждом изменении. Документ —
// файл из хранилища (`file`): байты читает worker ввода-вывода, сохраняет `saver`;
// уход к экрану файлов или другому файлу сначала сохраняет текущий (`onLeave`).

import { createEffect, createMemo, createSignal, ErrorBoundary, onCleanup, onMount, Show, untrack } from 'solid-js';
import { detectPlatform } from '../core/core';
import { NEW_FILE_TAKEN } from '../core/file-list';
import { MAX_JOURNAL_ENTRIES, MAX_OPEN_BYTES } from '../core/limits';
import { boot, dismissSplash, isTestMode } from './boot';
import { DesignHeader, FileHeader, PanelToggle, ResizeEdge } from './chrome';
import { buildContextMenu, type ContextTarget } from './context-menu';
import { EditorContext, type Editor } from './context';
import { ConfirmDialog } from './dialog';
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { buildMainMenu, entriesByKey, searchMenu } from './main-menu';
import { bindingsToCommands, parseRecent, pushRecent } from './palette';
import { LayersPanel } from './layers';
import { PagesPanel } from './pages';
import { PropertiesPanel } from './properties';
import { QuickActions } from './quick-actions';
import { diffSettings, parseSections, resolveTheme, type Settings, type ThemeChoice } from './settings';
import type { SettingsStore } from './settings-store';
import { SettingsDialog } from './settings-dialog';
import { ShortcutsPanel } from './shortcuts-panel';
import { downloadBytes, fileName } from './files';
import { importFromDisk, type FileMeta, type FileStore } from './file-store';
import { systemFonts } from './system-fonts';
import { isTauri, saveExport } from './tauri';
import type { Backend, MenuActionDto } from '../wasm/ferrum_wasm';
import { restartText, type Restart } from './restart';
import type { Route } from './router';
import { createSaver, SETTLE_MS, withTimeout, type Saver, type SaverOptions } from './saver';
import { clipboardAction, eventShortcut } from './shortcuts';
import { installClipboard } from './system-clipboard';
import { installTextInput } from './text-input';
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
  /** Дописать журнал без вопросов: вкладка сейчас перезагрузится (смена входа в другой вкладке). */
  onFlush?: (flush: () => Promise<boolean>) => void;
  /** Есть ли правки, которых нет ни в файле, ни в журнале: перезагрузка их потеряет. */
  onUnsaved?: (unsaved: () => Promise<boolean>) => void;
  /** Сессии больше нет (выход в другой вкладке), а файл не записался: как показать это человеку. */
  onSessionLost?: (mark: () => void) => void;
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
  /** Страница из ссылки (`?page=`): открыть её, если она есть в файле. */
  page?: string;
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
  const [leftCollapsed, setLeftCollapsed] = createSignal(saved.leftCollapsed);
  const [rightCollapsed, setRightCollapsed] = createSignal(saved.rightCollapsed);
  const [sections, setSections] = createSignal<ReadonlySet<string>>(new Set(parseSections(saved.collapsedSections)));
  const [themeChoice, setThemeChoice] = createSignal<ThemeChoice>(saved.theme);
  const [pixelSnap, setPixelSnap] = createSignal(saved.pixelSnap);
  // Настройка вида: команду выполняет ядро (событие `view`), хранит — интерфейс.
  const [rulers, setRulers] = createSignal(saved.rulers);
  const [guides, setGuides] = createSignal(saved.guides);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  /** Недавние команды «Быстрых действий»: новые первыми, общие для вкладок. */
  const [recent, setRecent] = createSignal(parseRecent(saved.recentCommands));
  /** Открытое контекстное меню: пункты из ядра, точка окна и название. */
  const [context, setContext] = createSignal<{ items: MenuItem[]; at: { x: number; y: number }; label: string } | null>(null);
  /** Запрошенное у ядра меню: куда его поставить и кто выполняет действия цели. */
  let pendingContext: { at: { x: number; y: number }; label: string; onAction?: (a: MenuActionDto) => void } | null = null;
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
    editor()?.send({ type: 'setTheme', theme: theme(), system: themeChoice() === 'system' });
  });
  createEffect(() => editor()?.send({ type: 'setSettings', pixelSnap: pixelSnap() }));
  // Показ линеек и направляющих, каким его знает ядро: пришедшее от него событие `view`
  // обратно командой не уходит (иначе два быстрых Shift+R дали бы лишний кадр).
  let coreView: { rulers: boolean; guides: boolean } | null = null;
  createEffect(() => {
    const v = { rulers: rulers(), guides: guides() };
    const ed = editor();
    if (!ed || (coreView && coreView.rulers === v.rulers && coreView.guides === v.guides)) return;
    coreView = v;
    ed.send({ type: 'setView', ...v });
  });
  // Холст держит в центре одну и ту же точку мира: сменилась ширина колонки панели —
  // центр холста ушёл на половину разницы, и содержимое поехало бы. Возвращаем его на
  // экранное место сдвигом вида на эту половину (команда `panView`, без наведения под
  // указателем: над холстом его сейчас нет); край, перетянутый мышью, ведёт себя
  // как прежде. 48 — `--panel-rail`.
  const RAIL = 48;
  let railL = leftCollapsed();
  let railR = rightCollapsed();
  createEffect(() => {
    const l = leftCollapsed();
    const r = rightCollapsed();
    untrack(() => {
      const dL = (l ? RAIL : left()) - (railL ? RAIL : left());
      const dR = (r ? RAIL : right()) - (railR ? RAIL : right());
      railL = l;
      railR = r;
      const dx = (dL - dR) / 2;
      if (dx !== 0 && !store.state.uiHidden) editor()?.send({ type: 'panView', dx });
    });
  });
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
  // (ширина и свёрнутость панелей и секций у каждой вкладки свои: последняя
  // записавшая вкладка выигрывает при следующем запуске).
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
    if (p.rulers !== undefined) {
      written = { ...written, rulers: p.rulers };
      setRulers(p.rulers);
    }
    if (p.guides !== undefined) {
      written = { ...written, guides: p.guides };
      setGuides(p.guides);
    }
    if (p.recentCommands !== undefined) {
      written = { ...written, recentCommands: p.recentCommands };
      setRecent(parseRecent(p.recentCommands));
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
      rulers: rulers(),
      guides: guides(),
      keymap: keymap(),
      leftWidth: left(),
      rightWidth: right(),
      pagesOpen: pagesOpen(),
      leftCollapsed: leftCollapsed(),
      rightCollapsed: rightCollapsed(),
      collapsedSections: [...sections()].join('\n'),
      recentCommands: recent().join(' '),
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
  /** Доступ к файлу потерян, пока он открыт (файл на сервере): снят или сессия кончилась. */
  const [lost, setLost] = createSignal<'none' | 'session' | 'diverged' | null>(null);
  /** Просьба о доступе из плашки «Нет доступа». */
  const [asked, setAsked] = createSignal(false);
  store.setState('fileRole', props.file.role ?? null);
  let offWatch: (() => void) | undefined;
  let saver: Saver | undefined;
  let offSaveKey: (() => void) | undefined;
  let disposed = false;
  // Байты файла читаются, пока запускается ядро. Не прочитались — к экрану файлов.
  let stored = false;
  /** Поколение файла при открытии: с него начинается проверка записи. */
  let version = props.file.version;
  /** Наибольший сеанс пачек журнала файла: сеанс этого редактора — после него. */
  let journalSession: number | null = null;
  /** Журнал есть, но не применён (открыт читателем): вернули право — файл открывается заново. */
  let journalHeld = false;
  const bytes = props.files.read(id).then(
    (r) => {
      stored = r.bytes !== null;
      version = r.meta.version;
      journalSession = r.session;
      journalHeld = r.journalHeld === true;
      store.setState('fileRole', r.meta.role ?? null);
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
  props.onFlush?.(async () => {
    // Не только журнал: у нового файла ещё нет записи, и журнал правку не сохранит.
    return saver ? withTimeout(saver.save(), SETTLE_MS, false) : true;
  });
  props.onUnsaved?.(async () => {
    if (!saver) return false;
    await withTimeout(saver.flushJournal(), SETTLE_MS, false);
    return saver.pending();
  });
  props.onSessionLost?.(() => {
    editor()?.send({ type: 'setViewOnly', on: true });
    setLost('session');
  });
  const [rescuing, setRescuing] = createSignal(false);
  /** Снова войти — это перезагрузка. Что не в журнале (новый файл без записи), с ней пропало бы. */
  const signInAgain = async () => {
    const s = saver;
    if (s) await withTimeout(s.flushJournal(), SETTLE_MS, false);
    if (s?.pending()) {
      setAskLeave({
        reason: 'Этот файл ещё ни разу не записан. Сначала сохраните копию на диск',
        answer: (go) => {
          if (!go) return;
          s?.release();
          location.reload();
        },
      });
      return;
    }
    location.reload();
  };
  const answerLeave = (go: boolean) => {
    const a = askLeave();
    setAskLeave(null);
    // Человек уже ответил: браузер не спрашивает второй раз, если за уходом идёт перезагрузка.
    if (go) saver?.release();
    a?.answer(go);
  };

  onMount(() => {
    boot(canvas, store, {
      document: bytes,
      backend: props.backend,
      systemFonts: systemFonts((blob) => props.files.fontBytes?.(blob) ?? Promise.reject(new Error('нет хранилища'))),
      // До документа: тема (первый кадр сразу в цвете темы), привязка и сохранённая раскладка.
      beforeDocument: (ed) => {
        ed.send({ type: 'setTheme', theme: theme(), system: themeChoice() === 'system' });
        ed.send({ type: 'setSettings', pixelSnap: pixelSnap() });
        coreView = { rulers: rulers(), guides: guides() };
        ed.send({ type: 'setView', ...coreView });
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
      // Ctrl/Cmd+C, X, V (и Shift+V — «Вставить поверх»), пока они вызывают
      // копирование, вырезание и вставку, идут
      // событиями буфера обмена (системный буфер), а не клавишами в ядро.
      isClipboardKey: (e) => {
        const s = eventShortcut(e);
        const action = s && clipboardAction(s, platform);
        if (!s || !action) return false;
        const key = { copy: 'edit.copy', cut: 'edit.cut', paste: 'edit.paste', pasteOver: 'edit.paste_over' }[action];
        const bound = !!store.state.keymap.commands.find((c) => c.key === key)?.bindings.includes(s);
        if (bound) noteKeyUsed(key);
        return bound;
      },
      onForwardedKey: (e) => {
        if (e.repeat) return;
        const s = eventShortcut(e);
        const key = s && bindingIndex().get(s);
        if (key) noteKeyUsed(key);
      },
    })
      .then((ed) => {
        if (disposed) {
          ed.dispose();
          return;
        }
        ed.uiCommand = uiCommand;
        ed.noteRecent = (key) => setRecent((r) => pushRecent(r, key));
        ed.sectionCollapsed = (name) => sections().has(name);
        ed.toggleSection = (name) =>
          setSections((list) => {
            const next = new Set(list);
            if (!next.delete(name)) next.add(name);
            return next;
          });
        ed.contextMenu = (target: ContextTarget, at, label, onAction) => {
          setContext(null);
          pendingContext = { at, label, onAction };
          ed.send({ type: 'contextMenu', target });
        };
        // Пункты — в той же пачке после выделения и состояний команд (ядро шлёт
        // меню последним): через очередь планировщика, вслед за ними.
        const offContext = ed.core.on((e) => {
          if (e.type !== 'contextMenu') return;
          ed.scheduler.enqueue(() => {
            const p = pendingContext;
            pendingContext = null;
            if (!p || e.items.length === 0) return;
            const items = buildContextMenu(e.items, entries(), {
              select: (id) => ed.send({ type: 'select', ids: [id], mode: 'replace' }),
              action: (a) => p.onAction?.(a),
            });
            setContext({ items, at: p.at, label: p.label });
          });
        });
        setEditor(ed);
        canvas.focus();
        const offClipboard = installClipboard(ed);
        // Поле правки текста — в сцене холста (под курсором текста).
        const offText = installTextInput(ed, canvas.parentElement ?? document.body, canvas);
        uninstall = () => {
          offContext();
          offText();
          offClipboard();
        };
        let diverged = false;
        saver = createSaver(ed, props.files, id, version, stored, {
          ...testIntervals(),
          journalSession,
          onDiverged: () => {
            diverged = true;
            ed.send({ type: 'setViewOnly', on: true });
            setLost('diverged');
          },
        });
        // Роль известна из чтения файла: читатель не правит и пока ответ опроса в пути.
        if (store.state.fileRole === 'viewer') ed.send({ type: 'setViewOnly', on: true });
        // Доступ к файлу меняется, пока он открыт. Этап 3 пришлёт то же событие по соединению файла.
        let toldNewer = false;
        let toldReopen = false;
        offWatch = props.files.watch?.(id, (state) => {
          // Этот документ в файл уже не ляжет, какими бы ни стали права: плашку снимает только уход.
          if (diverged) return;
          if (state.kind !== 'role') {
            // Правки — в журнале браузера, плашка предлагает копию на диск; уход спросит о несохранённом.
            ed.send({ type: 'setViewOnly', on: true });
            store.setState('fileCanShare', false);
            setLost(state.kind);
            return;
          }
          const before = store.state.fileRole;
          setLost(null);
          // Читателю вернули право, а его прежние правки ещё не в документе: первая запись прошла бы мимо
          // них. Файл открывается заново, журнал сливается. До этого режим просмотра.
          if (journalHeld && state.role !== 'viewer') {
            if (props.onRestart({ reason: 'role', lost: false, message: 'право правки вернули' })) return;
            if (!toldReopen) store.toast('Теперь вы можете редактировать файл. Откройте его заново, чтобы продолжить');
            toldReopen = true;
            return;
          }
          store.setState('fileRole', state.role);
          store.setState('fileCanShare', state.canShare);
          ed.send({ type: 'setViewOnly', on: state.role === 'viewer' });
          if (before !== state.role && state.role === 'viewer') {
            store.toast(
              store.state.modified
                ? 'Теперь вы только смотрите файл. Правки не записаны — сохраните копию на диск'
                : 'Теперь вы только смотрите файл',
            );
          } else if (before === 'viewer' && state.role !== 'viewer') store.toast('Теперь вы можете редактировать файл');
          // Своя запись, ответ на которую ещё в пути или потерян, — не «другой человек».
          if (saver && !state.own && state.version > saver.version() && !toldNewer) {
            toldNewer = true;
            store.toast('Файл изменён в другом окне или другим человеком. Откройте его заново, чтобы увидеть новую версию');
          }
        });
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
        // Страница из ссылки — по первому списку страниц после открытия; её нет в
        // файле (удалили) — файл остаётся на первой странице, без ошибки.
        let linkedPage = props.page;
        ed.core.on((e) => {
          if (e.type === 'uiCommand') uiCommand(e.key);
          if (e.type === 'view') {
            coreView = { rulers: e.rulers, guides: e.guides };
            setRulers(e.rulers);
            setGuides(e.guides);
          }
          if (!opening && linkedPage && e.type === 'pages') {
            const want = linkedPage;
            linkedPage = undefined;
            if (want !== e.current && e.pages.some((p) => p.id === want)) ed.send({ type: 'setPage', id: want });
          }
          if (e.type === 'recovered') recovered = true;
          if (opening && e.type === 'opened') {
            opening = false;
            if (props.restart) {
              // После потери GPU — рендер, который новое ядро действительно получило:
              // принудительный бэкенд, не запустившись, уступает CPU.
              const r =
                props.restart.reason === 'gpu' ? { ...props.restart, backend: ed.core.backend ?? props.restart.backend } : props.restart;
              store.toast(restartText(r), r.lost ? 'error' : 'info');
            } else if (recovered) store.toast('Восстановлены изменения, не записанные в файл');
            void props.files.touch(id).catch(() => {});
            // Имя файла — из хранилища (его могли переименовать на экране файлов);
            // тихо: открытие — не правка, файл из-за него не пересохраняется.
            if (e.name !== props.file.name) {
              // Читатель документ не меняет, даже тихо: имя — только для показа.
              if (store.state.fileRole === 'viewer') store.setState('documentName', props.file.name);
              else ed.send({ type: 'setDocumentName', name: props.file.name, quiet: true });
            }
          } else if (opening && stored && e.type === 'error' && e.message.startsWith('Не удалось открыть файл')) {
            opening = false;
            props.onOpenFailed(e.message);
          } else if (e.type === 'documentName') {
            // Переименовали в редакторе — запись в хранилище тоже (без ожидания снимка).
            props.files
              .rename(id, e.name)
              .catch((err: unknown) =>
                store.toast(`Не удалось переименовать файл: ${err instanceof Error ? err.message : String(err)}`, 'error'),
              );
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
            // Десктоп — родной диалог «Сохранить» (путь выбирает Rust), браузер — загрузка.
            if (isTauri()) {
              const name = e.name;
              saveExport(name, e.bytes)
                .then((saved) => saved && store.toast(`Экспортировано: ${saved}`))
                .catch((err: unknown) => store.toast(err instanceof Error ? err.message : String(err), 'error'));
            } else {
              downloadBytes(e.name, e.bytes, e.mime);
              store.toast(`Экспортировано: ${e.name}`);
            }
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
    offWatch?.();
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
  // Главное меню (спека подсистемы 2, раздел 4): дерево — из ядра, названия — из
  // таблицы команд. Файла на диске без File System Access нет: его пунктов тоже.
  const [mainMenu, setMainMenu] = createSignal(false);
  /** Открыты ли «Быстрые действия» (спека подсистемы 2, раздел 7). */
  const [palette, setPalette] = createSignal(false);
  /** Открыта ли панель сочетаний (раздел 8). */
  const [shortcuts, setShortcuts] = createSignal(false);
  /** Команды, вызванные сочетанием за этот сеанс (в настройки не пишутся): отмечены в
   * панели вместе с недавними. Клавиша сопоставляется по действующей раскладке; что
   * сделало ядро (например, ввод в правке текста), панели не важно. */
  const [keysUsed, setKeysUsed] = createSignal<ReadonlySet<string>>(new Set());
  const noteKeyUsed = (key: string) => {
    if (!keysUsed().has(key)) setKeysUsed(new Set([...keysUsed(), key]));
  };
  const bindingIndex = createMemo(() => bindingsToCommands(store.state.keymap.commands));
  const usedCommands = createMemo(() => new Set([...recent(), ...keysUsed()]));
  const hidden = createMemo(() => new Set(props.files.disk ? [] : ['ui.open_disk', 'ui.save_disk']));
  const entries = createMemo(() => entriesByKey(store.state.keymap.commands));
  const mainItems = () => buildMainMenu(store.state.mainMenu, entries(), hidden());
  const search: MenuSearch = {
    label: 'Поиск по командам',
    placeholder: 'Найти команду',
    results: (q) => searchMenu(store.state.mainMenu, entries(), q, hidden()),
  };
  // Команды интерфейса (`ui.*`): пункт меню вызывает их сразу, сочетание на холсте —
  // через ядро (событие `uiCommand`).
  const uiCommand = (key: string) => {
    const ed = editor();
    if (!ed) return;
    switch (key) {
      case 'ui.main_menu':
        setMainMenu(true);
        break;
      case 'ui.quick_actions':
        setPalette(true);
        break;
      case 'ui.new_file':
        props.navigate({ kind: 'new' });
        break;
      case 'ui.open':
        void openFromDisk();
        break;
      case 'ui.open_disk':
        if (props.files.disk) void openLinked();
        break;
      case 'ui.save_disk':
        if (props.files.disk) void saveToDisk();
        break;
      case 'ui.save_copy':
        void saveCopy(ed);
        break;
      case 'ui.export':
        // «Экспорт…»: к кнопке секции экспорта панели свойств (настройки — там же).
        // Панель и секция могли быть свёрнуты: сигналы вне пачки обновляют сразу.
        setRightCollapsed(false);
        setSections((list) => new Set([...list].filter((n) => n !== 'Экспорт')));
        document.querySelector<HTMLElement>('[data-export-run]')?.focus();
        break;
      case 'ui.all_files':
        props.navigate({ kind: 'files', section: 'recent' });
        break;
      case 'ui.settings':
        setSettingsOpen(true);
        break;
      case 'ui.shortcuts':
        // Как в Figma: то же сочетание закрывает панель.
        setShortcuts((v) => !v);
        break;
      case 'ui.about':
        store.toast('Ferrum — редактор векторной графики: ядро на Rust и WebAssembly, интерфейс — SolidJS');
        break;
      case 'ui.pixel_snap':
        setPixelSnap((v) => !v);
        break;
      case 'ui.theme_dark':
        setThemeChoice('dark');
        break;
      case 'ui.theme_light':
        setThemeChoice('light');
        break;
      case 'ui.theme_system':
        setThemeChoice('system');
        break;
    }
  };

  return (
    <ErrorBoundary
      fallback={(err: unknown) => (
        dismissSplash(true),
        (
          <div class="fe-boot fe-boot--error" role="alert">
            Ошибка интерфейса: {err instanceof Error ? err.message : String(err)}. Перезагрузите страницу
          </div>
        )
      )}
    >
      <div
        class="fe-app"
        classList={{ 'fe-app--bare': store.state.uiHidden }}
        style={{
          '--left-w': leftCollapsed() ? 'var(--panel-rail)' : `${left()}px`,
          '--right-w': rightCollapsed() ? 'var(--panel-rail)' : `${right()}px`,
        }}
      >
        <main class="fe-stage">
          <canvas
            ref={canvas}
            class="fe-canvas"
            tabindex="0"
            aria-label="Холст"
            onContextMenu={(e) => {
              // Точка холста — `offset*` (не чтение layout); окна — для меню.
              editor()?.contextMenu?.(
                { kind: 'canvas', x: e.offsetX, y: e.offsetY },
                { x: e.clientX, y: e.clientY },
                'Действия с выделением',
              );
            }}
          />
          <Show when={!store.state.ready && failure()}>
            <div class="fe-boot fe-boot--error" role="alert">
              {failure()}
            </div>
          </Show>
          <Show when={lost()}>
            {(kind) => (
              <div class="fe-boot fe-boot--error fe-boot--stopped fe-lost" role="alert">
                <strong>{kind() === 'none' ? 'Нет доступа' : kind() === 'diverged' ? 'Файл уже записан' : 'Сессия завершена'}</strong>
                <span>
                  {kind() === 'none'
                    ? 'Вам больше не открыт этот файл. Несохранённые правки можно сохранить копией на диск'
                    : kind() === 'diverged'
                      ? NEW_FILE_TAKEN
                      : 'Войдите снова, правки останутся в этом браузере'}
                </span>
                <span class="fe-lost__actions">
                  <button class="fe-btn fe-btn--secondary" onClick={() => editor()?.send({ type: 'save' })}>
                    Сохранить копию на диск
                  </button>
                  {kind() === 'diverged' ? (
                    <button
                      class="fe-btn fe-btn--primary"
                      disabled={rescuing()}
                      onClick={() => {
                        setRescuing(true);
                        void saver?.saveAsNew(`${store.state.documentName} (копия)`).then((meta) => {
                          setRescuing(false);
                          if (meta) props.navigate({ kind: 'file', id: meta.id });
                        });
                      }}
                    >
                      Сохранить новым файлом
                    </button>
                  ) : kind() === 'none' ? (
                    <>
                      <Show when={props.files.requestAccess && !asked()}>
                        <button
                          class="fe-btn fe-btn--secondary"
                          onClick={() => {
                            setAsked(true);
                            void props.files.requestAccess?.(id).catch(() => setAsked(false));
                          }}
                        >
                          Запросить доступ
                        </button>
                      </Show>
                      <button class="fe-btn fe-btn--primary" onClick={() => props.navigate({ kind: 'files', section: 'recent' })}>
                        К файлам
                      </button>
                    </>
                  ) : (
                    <button class="fe-btn fe-btn--primary" onClick={() => void signInAgain()}>
                      Войти
                    </button>
                  )}
                </span>
              </div>
            )}
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
                <Show when={palette()}>
                  <QuickActions hidden={hidden()} recent={recent()} onClose={() => setPalette(false)} />
                </Show>
              </EditorContext.Provider>
            )}
          </Show>
        </main>
        <Show when={editor()}>
          {(ed) => (
            <EditorContext.Provider value={ed()}>
              <aside class="fe-side fe-side--left" aria-label="Страницы и слои">
                <FileHeader
                  menu={mainItems}
                  search={search}
                  open={mainMenu()}
                  onOpen={setMainMenu}
                  onBack={() => props.navigate({ kind: 'files', section: 'recent' })}
                  location={props.file.projectId ? 'Проект' : props.file.role && props.file.role !== 'owner' ? 'Доступные мне' : undefined}
                  collapsed={leftCollapsed()}
                  onCollapse={setLeftCollapsed}
                />
                <Show when={!leftCollapsed()}>
                  <PagesPanel open={pagesOpen()} onToggle={setPagesOpen} fileId={id} />
                  <LayersPanel />
                  <ResizeEdge side="left" width={left()} onWidth={setLeft} />
                </Show>
              </aside>
              <aside class="fe-side fe-side--right" aria-label="Свойства">
                <div class="fe-side__toggle">
                  <PanelToggle side="right" collapsed={rightCollapsed()} onToggle={setRightCollapsed} />
                </div>
                <Show when={!rightCollapsed()}>
                  <DesignHeader file={props.file} />
                  <PropertiesPanel theme={theme()} />
                  <ResizeEdge side="right" width={right()} onWidth={setRight} />
                </Show>
              </aside>
              <TooltipLayer />
              <Show when={shortcuts()}>
                <ShortcutsPanel used={usedCommands()} onSettings={() => setSettingsOpen(true)} onClose={() => setShortcuts(false)} />
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
                {(m) => <Menu label={m().label} items={m().items} anchor={m().at} recents onClose={() => setContext(null)} />}
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
        <Toasts store={store} onUndo={(step) => editor()?.send({ type: 'undoStep', step })} />
      </div>
    </ErrorBoundary>
  );
}
