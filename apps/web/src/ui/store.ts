// Состояние интерфейса — только то, что прислало ядро: события применяются в
// фазе mutate планировщика (одна `batch()` на кадр), живые значения жеста — из
// общей памяти в фазе measure. Здесь ничего не вычисляется о документе.

import { createStore, reconcile } from 'solid-js/store';
import type {
  ConflictInfo,
  CoreEvent,
  KeymapEntry,
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
  canUndo: boolean;
  canRedo: boolean;
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
}

const initial: UiState = {
  ready: false,
  tool: 'move',
  selectionCount: 0,
  selectionIds: [],
  canUndo: false,
  canRedo: false,
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
};

/** Сколько уведомлений видно одновременно и сколько секунд живёт каждое. */
export const MAX_TOASTS = 3;
export const TOAST_MS = 6000;

export function createUiStore() {
  const [state, setState] = createStore<UiState>(structuredClone(initial));
  let toastId = 0;

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
        setState({ selectionCount: e.count, selectionIds: e.ids });
        break;
      case 'document':
        setState({ canUndo: e.canUndo, canRedo: e.canRedo });
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
        setState('keymap', {
          commands: e.commands,
          conflicts: e.conflicts,
          reserved: e.reserved,
          overrides: e.overrides,
        });
        break;
      case 'presets':
        setState('presets', e.presets);
        break;
      case 'toggleUi':
        setState('uiHidden', (h) => !h);
        break;
      case 'error':
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

  return { state, setState, apply, applyLive, toast };
}

export type UiStore = ReturnType<typeof createUiStore>;
