// Worker ядра: владеет редактором, документом, сценой и рендером (WasmCore).
// Кадр: забрать ввод (из кольца SAB или из сообщения кадра) → `input` → `frame`
// (рисует, только если нужно) → живое состояние → события.

import init, { WasmCore } from '../wasm/ferrum_wasm.js';
import type { CoreEvent } from '../wasm/ferrum_wasm';
import { Ctrl, CTRL_BYTES, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET } from '../core/layout.gen';
import { decodeText, encodeText, MAX_CLIPBOARD_BYTES } from '../core/clipboard-text';
import { MAX_IMAGE_SIDE } from '../core/limits';
import {
  clipboardBytesError,
  commandSizeError,
  copyTooLarge,
  isToWorker,
  pasteTextError,
  type ClipboardExtra,
  type FromWorker,
  type ToWorker,
} from '../core/messages';
import { imageSize } from '../core/image-size';
import { addSaturating, drainRing, holdInput, writeLive as writeLiveSlots, type Held } from '../core/shared';

const scope = self as unknown as DedicatedWorkerGlobalScope;

let core: WasmCore | null = null;
let sab: { ctrl: Int32Array; live: Float64Array; ring: Uint8Array } | null = null;
/** Записи, вычитанные из кольца за кадр (не больше ёмкости кольца). */
const drained = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
const early: ToWorker[] = [];
/** После фатальной ошибки worker молчит: входящее не копим и не разбираем. */
let dead = false;
/** `init` уже пришёл: второй не запускает ядро заново поверх первого. */
let started = false;
/** Асинхронный API не умеет свой формат: событие `clipboard` несёт и готовый текст
 * с меткой (base64 кодирует worker, не основной поток). */
let clipboardText = false;

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

/** Ошибка для пользователя без остановки ядра (неверное сообщение). */
function postError(message: string): void {
  post({ t: 'events', events: [{ type: 'error', message }] });
}


/** Картинка не PNG (JPEG, WebP из буфера обмена или файла) — перекодируется в
 * PNG здесь, в worker'е: ядро читает только PNG, а основной поток пиксели не трогает.
 * Размер проверяется по заголовку до декодирования: крошечный файл может объявить
 * гигапиксели. */
async function toPng(bytes: Uint8Array): Promise<Uint8Array> {
  const size = imageSize(bytes);
  if (!size) throw new Error('нужен PNG, JPEG или WebP');
  if (size.width > MAX_IMAGE_SIDE || size.height > MAX_IMAGE_SIDE) {
    throw new Error(`изображение больше ${MAX_IMAGE_SIDE} px`);
  }
  const bmp = await createImageBitmap(new Blob([bytes as BlobPart]));
  try {
    // Заголовок мог солгать: размер декодированной картинки — проверка ещё раз.
    if (bmp.width > MAX_IMAGE_SIDE || bmp.height > MAX_IMAGE_SIDE) {
      throw new Error(`изображение больше ${MAX_IMAGE_SIDE} px`);
    }
    const c = new OffscreenCanvas(bmp.width, bmp.height);
    c.getContext('2d')!.drawImage(bmp, 0, 0);
    return new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
  } finally {
    bmp.close();
  }
}

/** Пока картинка перекодируется, ядру не идёт ничего: следующие сообщения и записи
 * ввода (клавиши, указатель) ждут в одной очереди в порядке прихода, а кадры
 * рисуются как обычно. После перекодировки очередь воспроизводится по порядку:
 * вставка, затем Ctrl+Z — отменяется вставка, а не прошлая правка. Порядок — тот, в
 * котором worker получил вставку и ввод: основной поток сначала читает байты
 * картинки (`blob.arrayBuffer()`, миллисекунды) и только потом шлёт команду, так что
 * ввод в этот короткий промежуток законно идёт раньше вставки. Записи ввода в
 * очереди ограничены ёмкостью кольца: сверх неё теряются только движение и колесо. */
let converting = false;
const waiting: Held<ToWorker>[] = [];

/** Перекодировка закончилась: ждавшее — по порядку; новая перекодировка снова
 * останавливает очередь (оставшееся ждёт дальше). */
function release(): void {
  converting = false;
  while (waiting.length && !converting && core) {
    const h = waiting.shift()!;
    if ('msg' in h) handle(h.msg);
    else {
      try {
        core.input(h.input);
      } catch (e) {
        fatal(e);
      }
    }
  }
  step(null);
}

/** Команда ядру после проверки предела; ответы — сразу, кадр нарисуется по тику. */
function run(cmd: ToWorker & { t: 'cmd' }): void {
  // Предел размера проверяем и здесь: основной поток не доверенный.
  const error = commandSizeError(cmd.cmd);
  if (error) {
    postError(error);
    return;
  }
  core!.command(cmd.cmd);
  sendEvents();
}

/** Событие `clipboard` для основного потока: больше предела — отказ одной ошибкой
 * (вставка такого всё равно была бы отклонена); иначе — признак вырезания (в той же
 * пачке изменился документ) и, для запасного пути, готовый текст с меткой. */
function clipboardEvent(e: CoreEvent & { type: 'clipboard' }, cut: boolean): CoreEvent {
  if (e.bytes.length > MAX_CLIPBOARD_BYTES) return { type: 'error', message: copyTooLarge(e.bytes.length, cut) };
  const x = e as typeof e & ClipboardExtra;
  x.cut = cut;
  if (clipboardText) x.text = encodeText(e.bytes);
  return x;
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
  if (!core) return;
  let events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  if (events.some((e) => e.type === 'clipboard')) {
    const cut = events.some((e) => e.type === 'document');
    events = events.map((e) => (e.type === 'clipboard' ? clipboardEvent(e, cut) : e));
  }
  const transfer: Transferable[] = [];
  for (const e of events) {
    // У пустой пачки журнала байтов нет (`bytes` — undefined).
    if ('bytes' in e && e.bytes) transfer.push(e.bytes.buffer);
    if (e.type === 'pixels') transfer.push(e.rgba.buffer);
    // Снимок для хранилища: превью уходит вместе с байтами файла, тоже без копии.
    if (e.type === 'snapshot' && e.thumbnail) transfer.push(e.thumbnail.buffer);
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
    if (input && input.byteLength > 0) {
      if (converting) {
        const dropped = holdInput(waiting, input, RING_CAPACITY);
        if (dropped && sab) addSaturating(sab.ctrl, Ctrl.INPUT_DROPPED, dropped);
      } else core.input(input);
    }
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
  // Целиком под try: сбой в разборе чужого сообщения не уходит мимо `fatal`.
  try {
    switch (msg.t) {
      case 'cmd': {
        if (converting) {
          waiting.push({ msg });
          return;
        }
        const cmd = msg.cmd;
        // Команда не доверенная: не объект — её отвергнет разбор в ядре.
        const image = typeof cmd === 'object' && cmd !== null && (cmd.type === 'pasteImage' || cmd.type === 'placeImage');
        if (image && !commandSizeError(cmd) && imageSize(cmd.bytes)?.type !== 'png') {
          converting = true;
          toPng(cmd.bytes)
            .then(
              (png) => {
                if (!core) return;
                core.command({ ...cmd, bytes: png });
                sendEvents();
              },
              (e: unknown) => postError(`Не удалось прочитать изображение: ${e instanceof Error ? e.message : 'формат не поддерживается'}`),
            )
            .catch(fatal)
            .finally(release);
          return;
        }
        run(msg);
        break;
      }
      case 'pasteText': {
        if (converting) {
          waiting.push({ msg });
          return;
        }
        // Текст из буфера не доверенный: предел — до декодирования, испорченный
        // base64 — одна ошибка без правки.
        const error = pasteTextError(msg.text);
        if (error) {
          postError(error);
          return;
        }
        const bytes = decodeText(msg.text);
        if (!bytes || bytes.length === 0) {
          postError('Не удалось вставить: данные Ferrum в буфере обмена повреждены');
          return;
        }
        run({ t: 'cmd', cmd: { type: 'paste', bytes } });
        break;
      }
      case 'encodeClipboard': {
        // Запасной путь записи, когда браузер отказал асинхронному API: текст с
        // меткой — отсюда. Ядра не касается, поэтому не ждёт перекодировки.
        const error = clipboardBytesError(msg.bytes);
        if (error) postError(error);
        post({ t: 'clipboardText', text: error ? null : encodeText(msg.bytes) });
        break;
      }
      case 'tick':
        step(msg.input instanceof ArrayBuffer ? new Uint8Array(msg.input) : sab ? drain() : null);
        break;
      case 'init':
        break;
    }
  } catch (e) {
    fatal(e);
  }
}

// Сбои вне наших try/catch (отклонённые обещания, ошибки в обработчиках) тоже
// доходят до основного потока, а не остаются в консоли worker'а. `preventDefault`:
// иначе то же исключение дошло бы и до `worker.onerror` основного потока, и
// пользователь увидел бы о нём два сообщения.
scope.addEventListener('unhandledrejection', (e) => {
  e.preventDefault();
  fatal((e as PromiseRejectionEvent).reason);
});
scope.addEventListener('error', (e) => {
  e.preventDefault();
  fatal((e as ErrorEvent).error ?? (e as ErrorEvent).message);
});

scope.onmessage = async (e: MessageEvent<unknown>) => {
  if (dead) return;
  const msg = e.data;
  // Конверт не доверенный: не объект или неизвестный вид — ошибка, не сбой ядра.
  if (!isToWorker(msg)) {
    postError('Неверное сообщение для ядра');
    return;
  }
  if (msg.t !== 'init') {
    if (core) handle(msg);
    else early.push(msg);
    return;
  }
  if (started) {
    postError('Ядро уже запущено: повторный init пропущен');
    return;
  }
  started = true;
  clipboardText = msg.clipboardText === true;
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
