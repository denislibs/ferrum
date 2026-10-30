// Стенд этапа 4 (debug.html): холст на всё окно под управлением worker'а и
// отладочная плашка — для тестов ядра; редактор с панелями — index.html.
// Параметры адреса: `nodes` — размер страницы-примера (по умолчанию 1000),
// `backend` — webgpu | webgl | cpu, `transport=message` — без общей памяти,
// `atlas` — сторона единственной страницы атласа GPU (тесты вытеснения).
// `window.__ferrum` — для E2E-тестов.
//
// Буфер обмена здесь — внутренний (последние байты события `clipboard`):
// системный буфер, диалоги файлов и панели — в редакторе (src/ui).

import type { Backend, CoreEvent } from './wasm/ferrum_wasm';
import { Core } from './core/core';
import type { TransportMode } from './core/transport';
import { startHud } from './hud';

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('canvas') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLElement;

/** Параметр адреса из списка допустимых значений; иное — как будто не задан. */
function choice<T extends string>(name: string, allowed: readonly T[]): T | undefined {
  const v = params.get(name);
  return allowed.find((a) => a === v);
}

/** Сторона атласа — целое от 1 до 65535 (u16 в ядре); иное игнорируется. */
function atlasSide(): number | undefined {
  const n = Number(params.get('atlas'));
  return Number.isInteger(n) && n >= 1 && n <= 65535 ? n : undefined;
}

async function main(): Promise<void> {
  const core = await Core.start(canvas, {
    backend: choice<Backend>('backend', ['webgpu', 'webgl', 'cpu']),
    transport: choice<TransportMode>('transport', ['sab', 'message']),
    atlasSide: atlasSide(),
  });
  // Журнал событий для тестов (после `ready` — его забирает `Core.start`).
  const events: CoreEvent[] = [];
  window.__ferrum = { core, events, ready: false };
  let clipboard: Uint8Array | null = null;
  core.on((e) => {
    events.push(e);
    if (e.type === 'error') console.warn('ferrum:', e.message);
    if (e.type === 'opened') window.__ferrum!.ready = true;
    if (e.type === 'clipboard') clipboard = e.bytes;
    // Копия: байты уходят в worker с передачей буфера, свои остаются.
    if (e.type === 'pasteRequested' && clipboard) core.send({ type: 'paste', bytes: clipboard.slice() });
  });
  startHud(hud, core);
  // `?texts=N` — страница из N текстов (замер «10 000 текстов», подсистема 3).
  if (params.has('texts')) core.send({ type: 'demoText', texts: Number(params.get('texts')) });
  else core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 1000) });
  canvas.focus();
}

// Отказ `Core.start` уже начинается с «Ядро не запустилось: …».
main().catch((e: unknown) => {
  hud.textContent = e instanceof Error ? e.message : `Ядро не запустилось: ${String(e)}`;
});
