// Ядро для основного потока: worker с WasmCore, транспорт, пересылка ввода с
// холста и живое состояние. Основной поток ничего не считает о документе: он
// кладёт события ввода в буфер, раз в кадр будит worker и читает готовые значения.
// Интерфейс (этап 5) работает с ядром только через этот класс.

import type { Backend, Command, CoreEvent, CursorKind } from '../wasm/ferrum_wasm';
import * as enc from './input';
import { BACKENDS, Kind, LIVE_NAMES } from './layout.gen';
import { prepareCommand, type FromWorker } from './messages';
import { createTransport, type Transport, type TransportMode } from './transport';

export type LiveState = Record<(typeof LIVE_NAMES)[number], number>;

export type Platform = 'mac' | 'windows' | 'linux';

export interface CoreOptions {
  /** Принудительный бэкенд рендера; по умолчанию WebGPU → WebGL2 → CPU. */
  backend?: Backend;
  /** `message` — postMessage даже на изолированной странице (тесты). */
  transport?: TransportMode;
  platform?: Platform;
  /** Тесты: атлас изображений GPU из одной страницы этой стороны (вытеснение). */
  atlasSide?: number;
  /** Асинхронный Clipboard API не умеет свой формат: worker сам кодирует байты
   * буфера обмена текстом с меткой (событие `clipboard` несёт `text`). */
  clipboardText?: boolean;
  /** Пересылать ли клавишу редактору. По умолчанию — все, кроме сочетаний браузера;
   * интерфейс оставляет себе клавиши полей ввода, меню и диалогов и сочетания
   * буфера обмена (их ведут события `copy`/`cut`/`paste`). */
  keyFilter?: (e: KeyboardEvent) => boolean;
  /** Подписка до запуска: события, пришедшие вместе с `ready` (раскладка, пресеты),
   * иначе прошли бы раньше, чем вызывающий успеет вызвать `on`. */
  onEvent?: (e: CoreEvent) => void;
  /** Куда отдать чтение layout, которое Core делает после запуска (ResizeObserver,
   * смена плотности): планировщик интерфейса выполнит его в фазе measure. */
  measure?: (read: () => void) => void;
}

/** Сколько ждать события `ready` после запуска worker'а. */
export const START_TIMEOUT_MS = 20_000;

/** Все отказы `start` — с этим началом: плашка показывает текст как есть. */
const NOT_STARTED = 'Ядро не запустилось';

/** Время обработчиков основного потока по кадрам — для бюджета ≤ 4 мс. */
export interface MainThreadStats {
  frames: number;
  /** Самый дорогой кадр и 95-й процентиль, мс. */
  maxMs: number;
  p95Ms: number;
}

export function detectPlatform(): Platform {
  const p = navigator.platform.toLowerCase();
  if (p.startsWith('mac')) return 'mac';
  if (p.startsWith('win')) return 'windows';
  return 'linux';
}

/** Сочетания, которые оставляем браузеру (перезагрузка, инструменты разработчика). */
function browserKey(e: KeyboardEvent): boolean {
  if (/^F\d+$/.test(e.key)) return true;
  const mod = e.metaKey || e.ctrlKey;
  return mod && (e.code === 'KeyR' || e.code === 'KeyW' || e.code === 'KeyT' || (e.shiftKey && e.code === 'KeyI'));
}

/** CSS-курсор по курсору редактора (угол ресайза — к ближайшей стрелке). */
export function cssCursor(kind: CursorKind, angle: number): string {
  switch (kind) {
    case 'default':
      return 'default';
    case 'crosshair':
      return 'crosshair';
    case 'grab':
      return 'grab';
    case 'grabbing':
      return 'grabbing';
    case 'duplicate':
      return 'copy';
    case 'rotate':
      return 'alias';
    case 'resize': {
      const a = ((angle % 180) + 180) % 180;
      if (a < 22.5 || a >= 157.5) return 'ew-resize';
      if (a < 67.5) return 'nesw-resize';
      if (a < 112.5) return 'ns-resize';
      return 'nwse-resize';
    }
  }
}

export class Core {
  readonly mode: TransportMode;
  backend: Backend | null = null;
  fallbacks: string[] = [];
  live: LiveState | null = null;
  private readonly listeners = new Set<(e: CoreEvent) => void>();
  private rect = { left: 0, top: 0, width: 0, height: 0 };
  private raf = 0;
  private frameCost = 0;
  private readonly costs: number[] = [];
  private maxCost = 0;
  private disposers: (() => void)[] = [];
  private failed: ((e: Error) => void) | null = null;
  private readonly frameHooks = new Set<() => void>();
  private keyFilter: (e: KeyboardEvent) => boolean = () => true;
  private measure: (read: () => void) => void = (read) => read();
  /** Ошибки обработчиков кадра, о которых уже сообщили (одно сообщение — один раз). */
  private readonly hookErrors = new Set<string>();
  /** Ядро остановлено (сбой worker'а): сообщение было, кадров и ввода больше нет. */
  private stopped = false;
  /** Ожидающие ответа `encodeClipboard` — по порядку запросов. */
  private readonly encodes: ((text: string | null) => void)[] = [];

  private constructor(
    private readonly worker: Worker,
    private readonly transport: Transport,
    private readonly canvas: HTMLCanvasElement,
  ) {
    this.mode = transport.mode;
  }

  /** Запускает worker на холсте и ждёт события `ready` (не дольше
   * `START_TIMEOUT_MS`). Отказ — `Error` с текстом «Ядро не запустилось: …». */
  static async start(canvas: HTMLCanvasElement, opts: CoreOptions = {}): Promise<Core> {
    const worker = new Worker(new URL('../worker/core.worker.ts', import.meta.url), { type: 'module' });
    const transport = createTransport(worker, opts.transport);
    const core = new Core(worker, transport, canvas);
    if (opts.keyFilter) core.keyFilter = opts.keyFilter;
    if (opts.measure) core.measure = opts.measure;
    if (opts.onEvent) core.on(opts.onEvent);
    let timer = 0;
    const ready = new Promise<void>((resolve, reject) => {
      core.failed = reject;
      // Worker, который так и не ответил (завис на загрузке ядра), не должен
      // вешать запуск навсегда.
      timer = self.setTimeout(() => core.fail('истекло время ожидания'), START_TIMEOUT_MS);
      core.on((e) => {
        if (e.type === 'ready') {
          core.backend = e.backend;
          core.fallbacks = e.fallbacks;
          resolve();
        }
      });
    });
    transport.onMessage((m) => core.onWorker(m));
    // Сбои самого worker'а (не загрузился скрипт, исключение вне наших обработчиков).
    worker.onerror = (e) => {
      e.preventDefault?.();
      // Без текста приходит отказ загрузки или разбора самого скрипта.
      core.fail(e.message || (core.failed ? 'скрипт worker\'а не загрузился' : 'сбой worker\'а'));
    };
    worker.onmessageerror = () => core.fail('сообщение worker\'а не удалось разобрать');
    const r = canvas.getBoundingClientRect();
    core.rect = { left: r.left, top: r.top, width: r.width, height: r.height };
    const offscreen = canvas.transferControlToOffscreen();
    transport.post(
      {
        t: 'init',
        canvas: offscreen,
        sab: transport.sab,
        platform: opts.platform ?? detectPlatform(),
        width: Math.max(1, r.width),
        height: Math.max(1, r.height),
        dpr: devicePixelRatio,
        backend: opts.backend ?? null,
        atlas: opts.atlasSide ?? null,
        clipboardText: opts.clipboardText ?? false,
      },
      [offscreen],
    );
    try {
      await ready;
    } catch (e) {
      worker.terminate();
      throw e;
    } finally {
      clearTimeout(timer);
    }
    core.failed = null;
    core.listen();
    core.loop();
    return core;
  }

  private onWorker(m: FromWorker): void {
    if (m.t === 'events') {
      for (const e of m.events) {
        if (e.type === 'cursor') this.canvas.style.cursor = cssCursor(e.cursor, e.angle);
        for (const l of this.listeners) l(e);
      }
    } else if (m.t === 'fatal') {
      this.fail(m.message);
    } else if (m.t === 'clipboardText') {
      this.encodes.shift()?.(typeof m.text === 'string' ? m.text : null);
    }
  }

  /** Подписка на события ядра; возвращает отписку. */
  on(listener: (e: CoreEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Команда ядру. Байты (`Uint8Array`) передаются без копии. */
  send(cmd: Command): void {
    if (this.stopped) {
      this.emit({ type: 'error', message: 'Ядро остановлено, команда не выполнена' });
      return;
    }
    const prepared = prepareCommand(cmd);
    if ('error' in prepared) {
      this.emit({ type: 'error', message: prepared.error });
      return;
    }
    const error = this.transport.post({ t: 'cmd', cmd: prepared.cmd }, prepared.transfer);
    if (error) this.emit({ type: 'error', message: error });
  }

  /** Вставка текста с меткой из буфера обмена: текст уходит worker'у как есть, base64
   * декодирует он (предел длины — до отправки и ещё раз в worker'е). */
  pasteText(text: string): void {
    if (this.stopped) {
      this.emit({ type: 'error', message: 'Ядро остановлено, команда не выполнена' });
      return;
    }
    const error = this.transport.post({ t: 'pasteText', text });
    if (error) this.emit({ type: 'error', message: error });
  }

  /** Текст с меткой для байтов буфера обмена — кодирует worker (base64 десятков
   * мегабайт в основном потоке был бы длинной задачей). `null` — отказ (больше
   * предела — ошибка придёт событием) или ядро остановлено. Байты передаются. */
  encodeClipboard(bytes: Uint8Array): Promise<string | null> {
    if (this.stopped) return Promise.resolve(null);
    const own = bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength ? bytes : bytes.slice();
    return new Promise((resolve) => {
      const error = this.transport.post({ t: 'encodeClipboard', bytes: own }, [own.buffer as ArrayBuffer]);
      if (error) resolve(null);
      else this.encodes.push(resolve);
    });
  }

  private emit(e: CoreEvent): void {
    for (const l of this.listeners) l(e);
  }

  /** Отказ worker'а: до `ready` — `start` отклоняется, после — событие `error`.
   * Один раз: об одном сбое могут сообщить и worker (`fatal`), и `onerror`. После
   * него кадры не идут и ввод не пересылается. */
  private fail(message: string): void {
    if (this.stopped) return;
    this.stopped = true;
    for (const r of this.encodes.splice(0)) r(null);
    this.stop();
    if (this.failed) this.failed(new Error(`${NOT_STARTED}: ${message}`));
    else this.emit({ type: 'error', message: `Ядро остановлено: ${message}` });
  }

  /** Цикл кадров и слушатели ввода — снять. */
  private stop(): void {
    cancelAnimationFrame(this.raf);
    for (const d of this.disposers.splice(0)) d();
  }

  /** Бюджет основного потока: время обработчиков ввода и кадра. */
  mainThreadStats(): MainThreadStats {
    const sorted = [...this.costs].sort((a, b) => a - b);
    const p95 = sorted.length ? sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * 0.95))] : 0;
    return { frames: this.costs.length, maxMs: this.maxCost, p95Ms: p95 };
  }

  resetMainThreadStats(): void {
    this.costs.length = 0;
    this.maxCost = 0;
  }

  /** Отброшено событий ввода (очередь основного потока переполнилась). */
  droppedInput(): number {
    return this.transport.dropped();
  }

  /** Вызов в каждом кадре после тика транспорта и чтения живого состояния — внутри
   * замера бюджета (планировщик интерфейса). Возвращает отписку. */
  onFrame(hook: () => void): () => void {
    this.frameHooks.add(hook);
    return () => this.frameHooks.delete(hook);
  }

  /** Обработчик интерфейса с учётом его времени в бюджет кадра (прокрутка панелей,
   * перетаскивание полей). */
  costed<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
    return this.timed(fn);
  }

  /** Обработчик с учётом времени в бюджет кадра. */
  private timed<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
    return (...args: A) => {
      const t = performance.now();
      fn(...args);
      this.frameCost += performance.now() - t;
    };
  }

  private add<K extends keyof HTMLElementEventMap>(
    target: HTMLElement | Window,
    type: K,
    fn: (e: HTMLElementEventMap[K]) => void,
    options?: AddEventListenerOptions,
  ): void {
    const h = this.timed(fn) as EventListener;
    target.addEventListener(type, h, options);
    this.disposers.push(() => target.removeEventListener(type, h, options));
  }

  private listen(): void {
    const sink = this.transport.sink;
    const c = this.canvas;
    const xy = (e: { clientX: number; clientY: number }) => [e.clientX - this.rect.left, e.clientY - this.rect.top] as const;
    this.add(c, 'pointerdown', (e) => {
      c.setPointerCapture(e.pointerId);
      c.focus();
      // Кнопки «назад» и «вперёд» (3, 4) холсту не нужны.
      if (e.button > 2) return;
      const [x, y] = xy(e);
      enc.pointer(sink, Kind.POINTER_DOWN, e, x, y);
    });
    this.add(c, 'pointermove', (e) => {
      // Все промежуточные точки движения, а не одна на кадр.
      const all = e.getCoalescedEvents?.() ?? [];
      for (const p of all.length ? all : [e]) {
        const [x, y] = xy(p);
        enc.pointer(sink, Kind.POINTER_MOVE, p, x, y);
      }
    });
    this.add(c, 'pointerup', (e) => {
      if (e.button > 2) return;
      const [x, y] = xy(e);
      enc.pointer(sink, Kind.POINTER_UP, e, x, y);
    });
    this.add(c, 'pointercancel', () => enc.simple(sink, Kind.POINTER_CANCEL));
    this.add(
      c,
      'wheel',
      (e) => {
        e.preventDefault();
        const [x, y] = xy(e);
        enc.wheel(sink, e, x, y, this.rect.height);
      },
      { passive: false },
    );
    this.add(c, 'contextmenu', (e) => e.preventDefault());
    // Щипок тачпада в Safari приходит жестом, а не колесом с Ctrl.
    let gestureScale = 1;
    const gesture = (type: string, scale: number, e: Event) => {
      e.preventDefault();
      const ge = e as Event & { clientX: number; clientY: number };
      const [x, y] = xy(ge);
      if (type === 'gesturestart') gestureScale = 1;
      enc.pinch(sink, x, y, scale / gestureScale);
      gestureScale = scale;
    };
    for (const type of ['gesturestart', 'gesturechange'] as const) {
      const h = this.timed((e: Event) => gesture(type, (e as Event & { scale: number }).scale, e));
      c.addEventListener(type, h);
      this.disposers.push(() => c.removeEventListener(type, h));
    }
    const loneAlt = enc.createLoneAlt();
    this.add(window, 'keydown', (e) => {
      loneAlt.down(e);
      // Клавишу, которую уже обработал элемент интерфейса (preventDefault), ядру не отдаём.
      if (e.defaultPrevented || browserKey(e) || !this.keyFilter(e)) return;
      e.preventDefault();
      enc.key(sink, Kind.KEY_DOWN, e);
    });
    this.add(window, 'keyup', (e) => {
      // Одиночный Alt (измерения на холсте): на Windows его отпускание увело бы фокус
      // в меню браузера. Только когда клавиши у холста — в поле Alt остаётся браузеру.
      const active = document.activeElement;
      if (loneAlt.up(e) && (active === c || active === document.body || active === null)) e.preventDefault();
      // Отпускание — всегда: удержание (Space — рука, Alt — измерения) не должно
      // залипнуть, если фокус ушёл в поле, пока клавиша была нажата.
      if (!browserKey(e)) enc.key(sink, Kind.KEY_UP, e);
    });
    this.add(window, 'blur', () => {
      loneAlt.reset();
      enc.simple(sink, Kind.BLUR);
    });
    // Размер холста и dpr: ResizeObserver и смена плотности экрана.
    const resize = () => {
      const r = c.getBoundingClientRect();
      this.rect = { left: r.left, top: r.top, width: r.width, height: r.height };
      enc.resize(sink, Math.max(1, r.width), Math.max(1, r.height), devicePixelRatio);
    };
    // Чтение размера — через measure (если задан): до записи в DOM, без принудительной раскладки.
    const ro = new ResizeObserver(this.timed(() => this.measure(resize)));
    ro.observe(c);
    this.disposers.push(() => ro.disconnect());
    let mq: MediaQueryList | null = null;
    const watchDpr = () => {
      mq?.removeEventListener('change', onDpr);
      mq = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
      mq.addEventListener('change', onDpr);
    };
    const onDpr = this.timed(() => {
      this.measure(resize);
      watchDpr();
    });
    watchDpr();
    this.disposers.push(() => mq?.removeEventListener('change', onDpr));
  }

  private loop(): void {
    const frame = () => {
      if (this.stopped) return;
      const t = performance.now();
      this.transport.tick();
      const live = this.transport.live();
      if (live) {
        const obj = {} as LiveState;
        LIVE_NAMES.forEach((n, i) => (obj[n] = live[i]));
        this.live = obj;
      }
      for (const hook of this.frameHooks) {
        try {
          hook();
        } catch (e) {
          // Сбой интерфейса не должен остановить кадры ядра: сообщаем один раз.
          const message = `Ошибка интерфейса: ${e instanceof Error ? e.message : String(e)}`;
          if (!this.hookErrors.has(message)) {
            this.hookErrors.add(message);
            this.emit({ type: 'error', message });
          }
        }
      }
      this.frameCost += performance.now() - t;
      // Окно последних 10 000 кадров (около трёх минут при 60 Гц).
      if (this.costs.length === 10_000) this.costs.splice(0, 1000);
      this.costs.push(this.frameCost);
      this.maxCost = Math.max(this.maxCost, this.frameCost);
      this.frameCost = 0;
      this.raf = requestAnimationFrame(frame);
    };
    this.raf = requestAnimationFrame(frame);
  }

  /** Имя бэкенда по коду живого состояния. */
  static backendName(code: number): string {
    return BACKENDS[code] ?? 'none';
  }

  dispose(): void {
    this.stopped = true;
    for (const r of this.encodes.splice(0)) r(null);
    this.stop();
    this.worker.terminate();
  }
}
