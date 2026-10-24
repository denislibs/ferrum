// Стенд этапа 4: холст на всё окно под управлением worker'а и отладочная плашка.
// Параметры адреса: `nodes` — размер страницы-примера (по умолчанию 1000),
// `backend` — webgpu | webgl | cpu, `transport=message` — без общей памяти,
// `atlas` — сторона единственной страницы атласа GPU (тесты вытеснения).
// `window.__ferrum` — для E2E-тестов.
//
// Буфер обмена здесь — внутренний (последние байты события `clipboard`):
// системный буфер, диалоги файлов и панели — этап 5.

import type { Backend, CoreEvent } from './wasm/ferrum_wasm';
import { Core } from './core/core';
import type { TransportMode } from './core/transport';
import { startHud } from './hud';

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('canvas') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLElement;

async function main(): Promise<void> {
  const core = await Core.start(canvas, {
    backend: (params.get('backend') as Backend | null) ?? undefined,
    transport: (params.get('transport') as TransportMode | null) ?? undefined,
    atlasSide: params.has('atlas') ? Number(params.get('atlas')) : undefined,
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
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 1000) });
  canvas.focus();
}

main().catch((e: unknown) => {
  hud.textContent = `Ядро не запустилось: ${e instanceof Error ? e.message : String(e)}`;
});
