// Worker ядра: владеет редактором, документом, сценой и рендером (WasmCore).
// Кадр: забрать ввод (из кольца SAB или из сообщения кадра) → `input` → `frame`
// (рисует, только если нужно) → живое состояние → события.

import init, { WasmCore } from '../wasm/ferrum_wasm.js';
import type { CoreEvent } from '../wasm/ferrum_wasm';
import { Ctrl, CTRL_BYTES, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET } from '../core/layout.gen';
import { commandSizeError, type FromWorker, type ToWorker } from '../core/messages';
import { drainRing, writeLive as writeLiveSlots } from '../core/shared';

const scope = self as unknown as DedicatedWorkerGlobalScope;

let core: WasmCore | null = null;
let sab: { ctrl: Int32Array; live: Float64Array; ring: Uint8Array } | null = null;
/** Записи, вычитанные из кольца за кадр (не больше ёмкости кольца). */
const drained = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
const early: ToWorker[] = [];
/** После фатальной ошибки worker молчит: входящее не копим и не разбираем. */
let dead = false;

function post(msg: FromWorker, transfer: Transferable[] = []): void {
  scope.postMessage(msg, transfer);
}

function fatal(e: unknown): void {
  if (dead) return;
  dead = true;
  early.length = 0;
  post({ t: 'fatal', message: e instanceof Error ? e.message : String(e) });
  core = null;
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
  if (!core) return;
  const events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  const transfer: Transferable[] = [];
  for (const e of events) {
    if ('bytes' in e) transfer.push(e.bytes.buffer);
    if (e.type === 'pixels') transfer.push(e.rgba.buffer);
  }
  post({ t: 'events', events }, transfer);
}

/** Вычитывает записи кольца SAB (см. `drainRing` в shared.ts). */
function drain(): Uint8Array {
  const { count } = drainRing(sab!.ctrl, sab!.ring, drained);
  return drained.subarray(0, count * RECORD_SIZE);
}

function writeLive(): void {
  if (!core) return;
  const live = core.live();
  if (sab) {
    writeLiveSlots(sab.ctrl, sab.live, live);
  } else {
    post({ t: 'live', live }, [live.buffer]);
  }
}

function step(input: Uint8Array | null): void {
  if (!core) return;
  try {
    if (input && input.byteLength > 0) core.input(input);
    core.frame();
    writeLive();
    sendEvents();
  } catch (e) {
    fatal(e);
  }
}

/** Отдаёт управление циклу событий worker'а (команды — сообщения). */
const yieldTask = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

async function sabLoop(): Promise<void> {
  const s = sab!;
  let seen = Atomics.load(s.ctrl, Ctrl.FRAME);
  while (core) {
    const wait = Atomics.waitAsync(s.ctrl, Ctrl.FRAME, seen);
    // Кадр основного потока уже ушёл вперёд (прошлый кадр был долгим): ожидание
    // вернулось сразу. Без явной уступки цикл не дал бы дойти сообщениям — Save,
    // Open, Paste ждали бы, пока кадры не станут быстрыми.
    if (wait.async) await wait.value;
    else await yieldTask();
    seen = Atomics.load(s.ctrl, Ctrl.FRAME);
    try {
      step(drain());
    } catch (e) {
      fatal(e);
    }
  }
}

function handle(msg: ToWorker): void {
  if (!core) return;
  switch (msg.t) {
    case 'cmd':
      {
        // Предел размера проверяем и здесь: основной поток не доверенный.
        const error = commandSizeError(msg.cmd);
        if (error) {
          post({ t: 'events', events: [{ type: 'error', message: error }] });
          return;
        }
      }
      try {
        core.command(msg.cmd);
        // Ответы на команды (сохранение, ошибки) — сразу, кадр нарисуется по тику.
        sendEvents();
      } catch (e) {
        fatal(e);
      }
      break;
    case 'tick':
      try {
        step(msg.input ? new Uint8Array(msg.input) : sab ? drain() : null);
      } catch (e) {
        fatal(e);
      }
      break;
    case 'init':
      break;
  }
}

// Сбои вне наших try/catch (отклонённые обещания, ошибки в обработчиках) тоже
// доходят до основного потока, а не остаются в консоли worker'а.
scope.addEventListener('unhandledrejection', (e) => fatal((e as PromiseRejectionEvent).reason));
scope.addEventListener('error', (e) => fatal((e as ErrorEvent).error ?? (e as ErrorEvent).message));

scope.onmessage = async (e: MessageEvent<ToWorker>) => {
  if (dead) return;
  const msg = e.data;
  if (msg.t !== 'init') {
    if (core) handle(msg);
    else early.push(msg);
    return;
  }
  try {
    await init();
    core = await WasmCore.create(msg.canvas, msg.platform, msg.width, msg.height, msg.dpr, msg.backend, msg.atlas);
  } catch (err) {
    fatal(err);
    return;
  }
  const waitAsync = typeof Atomics.waitAsync === 'function';
  if (msg.sab) {
    sab = {
      ctrl: new Int32Array(msg.sab, 0, CTRL_BYTES / 4),
      live: new Float64Array(msg.sab, LIVE_OFFSET, LIVE_SLOTS),
      ring: new Uint8Array(msg.sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE),
    };
  }
  try {
    post({ t: 'started', waitAsync });
    writeLive();
    sendEvents();
  } catch (err) {
    fatal(err);
    return;
  }
  for (const m of early.splice(0)) handle(m);
  if (sab && waitAsync) void sabLoop();
};
