// Планировщик интерфейса (спека 4.4) — своя реализация идей fasterdom, без кода
// Telegram Web A. Один кадр — один вызов `frame()` из `requestAnimationFrame`
// ядра (`Core.onFrame`), поэтому время планировщика и обновлений Solid входит в
// бюджет основного потока, который меряет `Core`.
//
// Порядок кадра:
// 1. `measure` — чтения layout (размеры, прокрутка) и живого состояния из общей
//    памяти. DOM ещё не тронут в этом кадре, поэтому чтения не заставляют браузер
//    пересчитывать раскладку.
// 2. `mutate` — одна `batch()`: события ядра, пришедшие с прошлого кадра, затем то,
//    что вернули подписчики `everyFrame`, затем ручные записи в DOM. Solid
//    обновляет DOM один раз на кадр, сколько бы событий ни пришло.
//
// В режиме разработки `guardLayout` предупреждает о чтении layout вне `measure`.

export type Phase = 'idle' | 'measure' | 'mutate';

/** Подписчик кадра: читает в фазе measure и может вернуть запись для mutate. */
export type FrameReader = () => (() => void) | void;

export interface Scheduler {
  /** Событие ядра: применится в `batch()` следующего кадра. */
  enqueue(apply: () => void): void;
  /** Однократное чтение layout в ближайшей фазе measure. */
  measure(fn: () => void): void;
  /** Однократная запись в DOM в ближайшей фазе mutate (запрос из measure — в том же
   * кадре, из mutate — в следующем). */
  mutate(fn: () => void): void;
  /** Подписка на каждый кадр; возвращает отписку. */
  everyFrame(reader: FrameReader): () => void;
  /** Кадр: measure, затем mutate в одной `batch()`. */
  frame(): void;
  phase(): Phase;
}

export interface SchedulerOptions {
  /** `batch` из solid-js (в тестах — заглушка). */
  batch: (fn: () => void) => void;
  /** Ошибка подписчика не останавливает кадр: она уходит сюда. */
  onError?: (e: unknown) => void;
}

export function createScheduler(opts: SchedulerOptions): Scheduler {
  let phase: Phase = 'idle';
  let events: (() => void)[] = [];
  let measures: (() => void)[] = [];
  let mutates: (() => void)[] = [];
  const readers = new Set<FrameReader>();
  const onError = opts.onError ?? ((e: unknown) => console.error(e));
  const run = (fn: () => void) => {
    try {
      fn();
    } catch (e) {
      onError(e);
    }
  };
  return {
    enqueue(apply) {
      events.push(apply);
    },
    measure(fn) {
      measures.push(fn);
    },
    mutate(fn) {
      mutates.push(fn);
    },
    everyFrame(reader) {
      readers.add(reader);
      return () => readers.delete(reader);
    },
    phase: () => phase,
    frame() {
      phase = 'measure';
      const writes: (() => void)[] = [];
      for (const r of readers) {
        run(() => {
          const w = r();
          if (w) writes.push(w);
        });
      }
      // Чтения, запрошенные из чтений, — в этом же кадре.
      while (measures.length) {
        const batch = measures;
        measures = [];
        batch.forEach(run);
      }
      phase = 'mutate';
      const evs = events;
      events = [];
      const muts = mutates;
      mutates = [];
      opts.batch(() => {
        evs.forEach(run);
        writes.forEach(run);
        muts.forEach(run);
      });
      phase = 'idle';
    },
  };
}

/** Чтения layout, о которых предупреждает `guardLayout`. */
export const LAYOUT_READS = [
  'getBoundingClientRect',
  'getClientRects',
  'offsetWidth',
  'offsetHeight',
  'offsetTop',
  'offsetLeft',
  'clientWidth',
  'clientHeight',
  'scrollWidth',
  'scrollHeight',
  'scrollTop',
  'scrollLeft',
] as const;

/** Режим разработки: оборачивает методы и геттеры `proto` (Element.prototype,
 * HTMLElement.prototype) — чтение вне фазы measure даёт одно предупреждение на имя.
 * Возвращает снятие обёрток. */
export function guardLayout(
  protos: object[],
  scheduler: Pick<Scheduler, 'phase'>,
  warn: (message: string) => void = (m) => console.warn(m),
): () => void {
  const warned = new Set<string>();
  const restore: (() => void)[] = [];
  const check = (name: string) => {
    if (scheduler.phase() !== 'measure' && !warned.has(name)) {
      warned.add(name);
      warn(`Чтение layout «${name}» вне фазы measure: перенесите его в scheduler.measure`);
    }
  };
  for (const proto of protos) {
    for (const name of LAYOUT_READS) {
      const d = Object.getOwnPropertyDescriptor(proto, name);
      if (!d) continue;
      if (typeof d.value === 'function') {
        const f = d.value as (...a: unknown[]) => unknown;
        Object.defineProperty(proto, name, {
          ...d,
          value(this: unknown, ...a: unknown[]) {
            check(name);
            return f.apply(this, a);
          },
        });
      } else if (d.get) {
        const get = d.get;
        Object.defineProperty(proto, name, {
          ...d,
          get(this: unknown) {
            check(name);
            return get.call(this);
          },
        });
      } else {
        continue;
      }
      restore.push(() => Object.defineProperty(proto, name, d));
    }
  }
  return () => restore.forEach((r) => r());
}
