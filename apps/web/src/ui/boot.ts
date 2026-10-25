// Запуск редактора: ядро на холсте, события в состояние через планировщик,
// документ. Параметры адреса: `nodes` — страница-пример из N узлов (по умолчанию
// пустой документ), `backend`, `transport` — как у стенда (тесты).

import { batch, createSignal } from 'solid-js';
import type { Backend, CoreEvent } from '../wasm/ferrum_wasm';
import { Core, detectPlatform } from '../core/core';
import type { TransportMode } from '../core/transport';
import type { Editor } from './context';
import { supportsCustomFormat } from './clipboard';
import { forwardKey } from './keys';
import { createScheduler, guardLayout } from './scheduler';
import { loadLayoutMap, type LayoutMap } from './shortcuts';
import type { UiStore } from './store';

/** Журнал событий для тестов — последние записи, не больше этого числа. */
const EVENT_LOG = 2000;

/** Тестовый режим: E2E ставит `window.__ferrumTest` до загрузки или адрес с `?debug`. */
function testMode(params: URLSearchParams): boolean {
  return params.has('debug') || (window as { __ferrumTest?: boolean }).__ferrumTest === true;
}

/** Замер производительности (`window.__ferrumPerf` до загрузки): ядро и планировщик
 * доступны тесту, но без журнала событий и защиты чтений layout тестового режима —
 * они сами стоят времени основного потока. */
function perfHook(): boolean {
  return (window as { __ferrumPerf?: boolean }).__ferrumPerf === true;
}

/** Запись журнала: тип и размер, но не сами данные (картинки и буфер обмена — до 64 МБ). */
function note(e: CoreEvent): { type: string; size?: number; message?: string } {
  const n: { type: string; size?: number; message?: string } = { type: e.type };
  if ('bytes' in e && e.bytes instanceof Uint8Array) n.size = e.bytes.length;
  if (e.type === 'error') n.message = e.message;
  return n;
}

function choice<T extends string>(params: URLSearchParams, name: string, allowed: readonly T[]): T | undefined {
  const v = params.get(name);
  return allowed.find((a) => a === v);
}

export interface BootOptions {
  /** Клавиша — сочетание буфера обмена, которое ведут события copy/cut/paste. */
  isClipboardKey?: (e: KeyboardEvent) => boolean;
  /** Перед документом: настройки пользователя (тема, раскладка, привязка) — до первого кадра. */
  beforeDocument?: (editor: Editor) => void;
}

export async function boot(canvas: HTMLCanvasElement, store: UiStore, opts: BootOptions = {}): Promise<Editor> {
  const params = new URLSearchParams(location.search);
  const platform = detectPlatform();
  const test = testMode(params);
  const reported = new Set<string>();
  const scheduler = createScheduler({
    batch,
    dev: import.meta.env.DEV || test,
    // Ошибка интерфейса в кадре: одно уведомление на каждое сообщение, не в цикле.
    onError: (e) => {
      console.error(e);
      const message = `Ошибка интерфейса: ${e instanceof Error ? e.message : String(e)}`;
      if (reported.has(message)) return;
      reported.add(message);
      store.toast(message, 'error');
    },
  });
  const uiEvents: { type: string; size?: number; message?: string }[] = [];
  const isClipboardKey = opts.isClipboardKey ?? (() => false);
  const core = await Core.start(canvas, {
    platform,
    backend: choice<Backend>(params, 'backend', ['webgpu', 'webgl', 'cpu']),
    transport: choice<TransportMode>(params, 'transport', ['sab', 'message']),
    keyFilter: (e) => forwardKey(e, isClipboardKey),
    // Свой формат не поддерживается: текст с меткой для буфера готовит worker.
    clipboardText: !supportsCustomFormat(),
    // Чтения layout самого Core — в фазе measure планировщика.
    measure: (read) => scheduler.measure(read),
    onEvent: (e) => {
      if (test) {
        uiEvents.push(note(e));
        if (uiEvents.length > EVENT_LOG) uiEvents.splice(0, EVENT_LOG / 2);
      }
      if (e.type === 'opened' && window.__ferrum) window.__ferrum.ready = true;
      // Ошибка — сразу, мимо планировщика: при остановке ядра кадров больше нет, и
      // из очереди сообщение не дошло бы до пользователя никогда.
      if (e.type === 'error') store.apply(e);
      else scheduler.enqueue(() => store.apply(e));
    },
  });
  // Защита ставится после запуска: стартовые чтения Core — не нарушение. Предупреждения
  // в тестовом режиме копятся в `window.__ferrumWarnings`.
  if (import.meta.env.DEV || test) {
    guardLayout([Element.prototype, HTMLElement.prototype], scheduler, (m) => {
      console.warn(m);
      ((window as { __ferrumWarnings?: string[] }).__ferrumWarnings ??= []).push(m);
    });
  }
  core.onFrame(() => scheduler.frame());
  scheduler.everyFrame(() => {
    const l = core.live;
    if (l) return () => store.applyLive(l);
  });
  // Ядро и планировщик — только тестам и замерам: в обычном режиме страница их не
  // раскрывает (ни скриптам, ни расширениям). Готовность и состояние интерфейса — всегда.
  const expose = test || perfHook();
  window.__ferrum = {
    core: expose ? core : undefined,
    events: [],
    uiEvents: test ? uiEvents : undefined,
    ready: false,
    ui: store.state,
  } as unknown as NonNullable<Window['__ferrum']>;
  if (expose) (window.__ferrum as unknown as { scheduler: typeof scheduler }).scheduler = scheduler;
  // Раскладка клавиатуры для подписей: приходит позже запуска и может смениться.
  const [layout, setLayout] = createSignal<LayoutMap>();
  const refreshLayout = () => void loadLayoutMap().then((m) => setLayout(() => m));
  refreshLayout();
  (navigator as Navigator & { keyboard?: EventTarget }).keyboard?.addEventListener?.('layoutchange', refreshLayout);
  const editor: Editor = {
    core,
    store,
    scheduler,
    platform,
    send: (cmd) => core.send(cmd),
    run: (command) => core.send({ type: 'execute', command }),
    layout,
  };
  opts.beforeDocument?.(editor);
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
  return editor;
}
