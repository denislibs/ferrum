// Запуск редактора: ядро на холсте, события в состояние через планировщик,
// документ — байты файла из хранилища или, у нового файла, пустой документ.
// Параметры адреса: `nodes` — новый файл со страницей-примером из N узлов
// (тесты и замеры), `backend`, `transport` — как у стенда (тесты).

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

/** Сколько заставка держится с открытия страницы, даже если ядро готово раньше:
 *  за это время рамка успевает нарисоваться, получить ручки и залиться цветом. */
export const MIN_SPLASH_MS = 2000;

/** Убирает экран запуска из index.html: плавно гасит и удаляет из DOM. Безопасно
 *  вызывать много раз и без экрана (стенд, тесты). Ошибку показываем сразу; без
 *  анимации (prefers-reduced-motion) и в автотестах ждать нечего. Экран файлов
 *  тоже убирает её сразу: он не ждёт ядра, первый показ — быстрый. */
let splashTimer = 0;

export function dismissSplash(immediate = false) {
  const el = document.getElementById('fe-splash');
  if (!el || el.classList.contains('fe-splash--done')) return;
  const quick =
    immediate ||
    (window as { __ferrumTest?: boolean }).__ferrumTest === true ||
    matchMedia('(prefers-reduced-motion: reduce)').matches;
  // Уже ждёт своих 2 с, а теперь — сразу (ошибка, уход на экран файлов): ускорить.
  if (el.dataset.leaving && !quick) return;
  el.dataset.leaving = '1';
  clearTimeout(splashTimer);
  const wait = quick ? 0 : Math.max(0, MIN_SPLASH_MS - performance.now());
  splashTimer = window.setTimeout(() => leave(el), wait);
}

function leave(el: HTMLElement) {
  el.classList.add('fe-splash--done');
  el.removeAttribute('role');
  const remove = () => el.remove();
  el.addEventListener('transitionend', remove, { once: true });
  setTimeout(remove, 400); // без перехода (prefers-reduced-motion) transitionend не придёт
}

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
  /** Байты файла (читаются, пока запускается ядро); `null` — новый файл. */
  document?: Promise<ArrayBuffer | null>;
}

/** Тестовый режим страницы (`__ferrumTest` или `?debug`). */
export function isTestMode(): boolean {
  return testMode(new URLSearchParams(location.search));
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
  let unguard = () => {};
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
    unguard = guardLayout([Element.prototype, HTMLElement.prototype], scheduler, (m) => {
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
    scheduler: expose ? scheduler : undefined,
    events: [],
    uiEvents: test ? uiEvents : undefined,
    ready: false,
    ui: store.state,
  };
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
    // Ядро и защита чтений layout уходят вместе с редактором: экрану файлов они не нужны.
    dispose: () => {
      unguard();
      core.dispose();
    },
  };
  opts.beforeDocument?.(editor);
  const bytes = (await opts.document) ?? null;
  if (bytes) core.send({ type: 'open', bytes: new Uint8Array(bytes) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
  return editor;
}
