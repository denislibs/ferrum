// Запуск редактора: ядро на холсте, события в состояние через планировщик,
// документ. Параметры адреса: `nodes` — страница-пример из N узлов (по умолчанию
// пустой документ), `backend`, `transport` — как у стенда (тесты).

import { batch } from 'solid-js';
import type { Backend, CoreEvent } from '../wasm/ferrum_wasm';
import { Core, detectPlatform } from '../core/core';
import type { TransportMode } from '../core/transport';
import type { Editor } from './context';
import { forwardKey } from './keys';
import { createScheduler, guardLayout } from './scheduler';
import type { UiStore } from './store';

/** Журнал событий для тестов — последние события, не больше этого числа. */
const EVENT_LOG = 2000;

function choice<T extends string>(params: URLSearchParams, name: string, allowed: readonly T[]): T | undefined {
  const v = params.get(name);
  return allowed.find((a) => a === v);
}

export interface BootOptions {
  /** Клавиша — сочетание буфера обмена, которое ведут события copy/cut/paste. */
  isClipboardKey?: (e: KeyboardEvent) => boolean;
}

export async function boot(canvas: HTMLCanvasElement, store: UiStore, opts: BootOptions = {}): Promise<Editor> {
  const params = new URLSearchParams(location.search);
  const platform = detectPlatform();
  const scheduler = createScheduler({ batch });
  if (import.meta.env.DEV) guardLayout([Element.prototype, HTMLElement.prototype], scheduler);
  const events: CoreEvent[] = [];
  const isClipboardKey = opts.isClipboardKey ?? (() => false);
  const core = await Core.start(canvas, {
    platform,
    backend: choice<Backend>(params, 'backend', ['webgpu', 'webgl', 'cpu']),
    transport: choice<TransportMode>(params, 'transport', ['sab', 'message']),
    keyFilter: (e) => forwardKey(e, isClipboardKey),
    onEvent: (e) => {
      events.push(e);
      if (events.length > EVENT_LOG) events.splice(0, EVENT_LOG / 2);
      if (e.type === 'opened' && window.__ferrum) window.__ferrum.ready = true;
      // Ошибка — сразу, мимо планировщика: при остановке ядра кадров больше нет, и
      // из очереди сообщение не дошло бы до пользователя никогда.
      if (e.type === 'error') store.apply(e);
      else scheduler.enqueue(() => store.apply(e));
    },
  });
  core.onFrame(() => scheduler.frame());
  scheduler.everyFrame(() => {
    const l = core.live;
    if (l) return () => store.applyLive(l);
  });
  window.__ferrum = { core, events, ready: false, ui: store.state };
  const editor: Editor = {
    core,
    store,
    scheduler,
    platform,
    send: (cmd) => core.send(cmd),
    run: (command) => core.send({ type: 'execute', command }),
  };
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 0) });
  return editor;
}
