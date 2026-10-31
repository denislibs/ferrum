// Состояние интерфейса — только то, что прислало ядро: события применяются в
// фазе mutate планировщика (одна `batch()` на кадр), живые значения жеста — из
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import { createStore, reconcile } from 'solid-js/store';
import type {
  ConflictInfo,
  CoreEvent,
  FontChoice,
  KeymapEntry,
  TextStyleDto,
  MissingFontDto,
  LayerRow,
  PageInfo,
  PresetInfo,
  Summary,
  Tool,
} from '../wasm/ferrum_wasm';
import type { LiveState } from '../core/core';

/** Уведомление: текст и вид (ошибка — с иконкой и словом). */
export interface Toast {
  id: number;
  text: string;
  kind: 'error' | 'info';
}

export interface UiState {
  ready: boolean;
  tool: Tool;
  selectionCount: number;
  /** Идентификаторы выделенных (не больше, чем в событии ядра): адресат правок панели. */
  selectionIds: string[];
  /** Версия выделения ядра: правка без id (выделение больше предела id) действует, только пока она та же. */
  selectionVersion: number;
  canUndo: boolean;
  canRedo: boolean;
  /** Документ изменён с открытия, создания или сохранения — и правками вне истории
   * отмены (страницы, фон, имя файла). «Создать» и «Открыть…» спрашивают по нему. */
  modified: boolean;
  documentName: string;
  layers: { start: number; total: number; rows: LayerRow[]; reveal: number | null; revealSeq: number };
  summary: Summary | null;
  pages: { list: PageInfo[]; current: string; background: string | null };
  keymap: { commands: KeymapEntry[]; conflicts: ConflictInfo[]; reserved: string[]; overrides: string };
  presets: PresetInfo[];
  /** Ядро остановлено (сбой worker'а): причина; интерфейс показывает её поверх холста. */
  stopped: string | null;
  /** Живое состояние, нужное панелям во время жеста. */
  live: { zoom: number; busy: boolean; x: number; y: number; w: number; h: number; rotation: number; count: number };
  uiHidden: boolean;
  /** Сколько раз открывали документ (сброс поиска и прокрутки панели слоёв). */
  docSeq: number;
  toasts: Toast[];
  /** Сочетание занято другими командами: настройки предлагают заменить. */
  bindConflict: { command: string; shortcut: string; commands: string[] } | null;
  /** Отсутствующие шрифты текущей страницы (подсистема 3) и семейства для замены. */
  missingFonts: MissingFontDto[];
  fontCatalog: FontChoice[];
  /** Системные шрифты: есть ли на платформе и показаны ли уже. */
  systemFonts: { supported: boolean; shown: boolean };
  /** Правка текста на холсте (подсистема 3, этап 2): идёт ли и новый ли текст. */
  text: { editing: boolean; draft: boolean };
  /** Стиль для секции «Текст»; `null` — текстов не выделено. */
  textStyle: TextStyleDto | null;
  /** Семейства текстов текущей страницы (группа «В документе» выбора шрифта). */
  pageFonts: string[];
}

const initial: UiState = {
  ready: false,
  tool: 'move',
  selectionCount: 0,
  selectionIds: [],
  selectionVersion: 0,
  canUndo: false,
  canRedo: false,
  modified: false,
  documentName: '',
  layers: { start: 0, total: 0, rows: [], reveal: null, revealSeq: 0 },
  summary: null,
  pages: { list: [], current: '', background: null },
  keymap: { commands: [], conflicts: [], reserved: [], overrides: '' },
  presets: [],
  stopped: null,
  live: { zoom: 1, busy: false, x: 0, y: 0, w: 0, h: 0, rotation: 0, count: 0 },
  uiHidden: false,
  docSeq: 0,
  toasts: [],
  bindConflict: null,
  missingFonts: [],
  fontCatalog: [],
  systemFonts: { supported: false, shown: false },
  text: { editing: false, draft: false },
  textStyle: null,
  pageFonts: [],
};

/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
export const MAX_TOASTS = 3;
export const TOAST_MS = 6000;

export function createUiStore() {
  const [state, setState] = createStore<UiState>(structuredClone(initial));
  let toastId = 0;
  /** Сохранённая раскладка при запуске: ждём её ответа (`keymapImported` или `error`). */
  let quiet: { onDone?: () => void; onFail?: () => void } | null = null;

  function toast(text: string, kind: Toast['kind'] = 'info'): void {
    // То же сообщение подряд — одно уведомление, а не стопка.
    if (state.toasts.some((t) => t.text === text)) return;
    const id = ++toastId;
    setState('toasts', (list) => [...list, { id, text, kind }].slice(-MAX_TOASTS));
    setTimeout(() => setState('toasts', (list) => list.filter((t) => t.id !== id)), TOAST_MS);
  }

  /** Событие ядра → состояние. Вызывается внутри `batch()` планировщика. */
  function apply(e: CoreEvent): void {
    switch (e.type) {
      case 'opened':
        setState({ ready: true, documentName: e.name });
        setState('docSeq', (n) => n + 1);
        for (const w of e.warnings) toast(w, 'error');
        break;
      case 'documentName':
        setState('documentName', e.name);
        break;
      case 'tool':
        setState('tool', e.tool);
        break;
      case 'selection':
        setState({ selectionCount: e.count, selectionIds: e.ids, selectionVersion: e.version });
        break;
      case 'document':
        setState({ canUndo: e.canUndo, canRedo: e.canRedo });
        break;
      case 'modified':
        setState('modified', e.modified);
        break;
      case 'layers':
        // Строки сверяются по id: Solid перерисовывает только изменившиеся.
        setState('layers', 'rows', reconcile(e.rows, { key: 'id' }));
        setState('layers', { start: e.start, total: e.total, reveal: e.reveal ?? null });
        if (e.reveal != null) setState('layers', 'revealSeq', (n) => n + 1);
        break;
      case 'properties':
        setState('summary', reconcile(e.summary));
        break;
      case 'pages':
        setState('pages', { list: e.pages, current: e.current, background: e.background ?? null });
        break;
      case 'keymap':
        // Команды сверяются по ключу: объект строки сохраняется, и таблица настроек не
        // пересоздаёт строки (фокус на кнопке строки не теряется при каждой правке).
        setState('keymap', 'commands', reconcile(e.commands, { key: 'key' }));
        setState('keymap', { conflicts: e.conflicts, reserved: e.reserved, overrides: e.overrides });
        break;
      case 'presets':
        setState('presets', e.presets);
        break;
      case 'bindConflict':
        setState('bindConflict', { command: e.command, shortcut: e.shortcut, commands: e.commands });
        break;
      case 'keymapImported': {
        // Сохранённая раскладка при запуске — молча; пропущенное — одним уведомлением.
        const silent = quiet !== null;
        const done = quiet?.onDone;
        quiet = null;
        if (silent) {
          done?.();
          if (e.warnings.length) toast(`Сохранённая раскладка: пропущено ${e.warnings.length}`, 'error');
          break;
        }
        toast(
          e.warnings.length
            ? `Раскладка импортирована, пропущено: ${e.warnings.length} (${e.warnings.slice(0, 2).join('; ')})`
            : 'Раскладка импортирована',
        );
        break;
      }
      case 'toggleUi':
        setState('uiHidden', (h) => !h);
        break;
      case 'missingFonts':
        setState('missingFonts', reconcile(e.list));
        break;
      case 'fontCatalog':
        setState('fontCatalog', e.families);
        break;
      case 'textCaret':
        setState('text', { editing: e.editing, draft: e.draft });
        break;
      case 'textStyleSummary':
        setState('textStyle', e.summary ? reconcile(e.summary) : null);
        break;
      case 'pageFonts':
        setState('pageFonts', e.families);
        break;
      case 'error':
        // Сохранённая раскладка не принята: ошибка показана, а в хранилище она не остаётся.
        if (quiet && e.message.startsWith('Не удалось импортировать раскладку')) {
          const { onFail } = quiet;
          quiet = null;
          onFail?.();
        }
        // «Ядро остановлено: …» — одно сообщение навсегда, а не уведомление.
        // Любое сообщение об остановке («…: причина», «…, команда не выполнена»)
        // уже показано плашкой: повторных уведомлений на каждую команду нет.
        if (e.message.startsWith('Ядро остановлено')) {
          if (state.stopped === null && e.message.startsWith('Ядро остановлено:')) setState('stopped', e.message);
        } else toast(e.message, 'error');
        break;
      default:
        break;
    }
  }

  /** Живое состояние кадра → поля панели (только изменившиеся). */
  function applyLive(l: LiveState): void {
    setState('live', {
      zoom: l.zoom,
      busy: l.busy === 1,
      x: l.selectionX,
      y: l.selectionY,
      w: l.selectionW,
      h: l.selectionH,
      rotation: l.selectionRotation,
      count: l.selectionCount,
    });
  }

  /** Следующий импорт раскладки — сохранённая при запуске: без уведомления; `onDone` — ядро
   * приняло (событие раскладки с очищенным набором придёт следом), `onFail` — не приняло. */
  function quietKeymapImport(onDone?: () => void, onFail?: () => void): void {
    quiet = { onDone, onFail };
  }

  return { state, setState, apply, applyLive, toast, quietKeymapImport };
}

export type UiStore = ReturnType<typeof createUiStore>;
