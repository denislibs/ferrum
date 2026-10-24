// Проверки без браузера (Node): кольцо ввода, замок живого состояния, предел
// размера команд, запасной транспорт на postMessage и сверка байтов записей с
// Rust. Идут в проекте `chromium` как обычные тесты Playwright без страницы.

import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import * as enc from '../src/core/input';
import { Ctrl, Kind, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET, SAB_BYTES } from '../src/core/layout.gen';
import { commandSizeError, MAX_COMMAND_BYTES, type FromWorker, type ToWorker } from '../src/core/messages';
import { drainRing, LIVE_READ_RETRIES, LiveReader, writeLive } from '../src/core/shared';
import { MessageTransport, SabTransport } from '../src/core/transport';

const MiB = 1024 * 1024;

function sharedParts() {
  const sab = new SharedArrayBuffer(SAB_BYTES);
  return {
    sab,
    ctrl: new Int32Array(sab, 0, 16),
    live: new Float64Array(sab, LIVE_OFFSET, LIVE_SLOTS),
    ring: new Uint8Array(sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE),
  };
}

/** Кладёт в слот кольца запись с номером `id` в первых четырёх байтах. */
function put(ring: Uint8Array, counter: number, id: number): void {
  const at = ((counter >>> 0) % RING_CAPACITY) * RECORD_SIZE;
  new DataView(ring.buffer, ring.byteOffset + at, RECORD_SIZE).setUint32(0, id, true);
}

function ids(out: Uint8Array, count: number): number[] {
  const view = new DataView(out.buffer, out.byteOffset);
  return Array.from({ length: count }, (_, i) => view.getUint32(i * RECORD_SIZE, true));
}

test.describe('кольцо ввода', () => {
  test('читает записи по порядку и сдвигает счётчик чтения', () => {
    const { ctrl, ring } = sharedParts();
    const out = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
    // Начало не с нуля: записи переходят через конец кольца.
    const start = RING_CAPACITY - 3;
    Atomics.store(ctrl, Ctrl.INPUT_READ, start);
    for (let i = 0; i < 8; i++) put(ring, start + i, 100 + i);
    Atomics.store(ctrl, Ctrl.INPUT_WRITE, start + 8);
    const r = drainRing(ctrl, ring, out);
    expect(r).toEqual({ count: 8, excess: 0 });
    expect(ids(out, 8)).toEqual([100, 101, 102, 103, 104, 105, 106, 107]);
    expect(Atomics.load(ctrl, Ctrl.INPUT_READ)).toBe(start + 8);
    expect(drainRing(ctrl, ring, out)).toEqual({ count: 0, excess: 0 });
  });

  test('счётчики переходят через 2^32', () => {
    const { ctrl, ring } = sharedParts();
    const out = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
    // int32: чтение −8 (0xFFFFFFF8), запись +8 — шестнадцать записей через границу.
    Atomics.store(ctrl, Ctrl.INPUT_READ, -8);
    for (let i = 0; i < 16; i++) put(ring, -8 + i, 500 + i);
    Atomics.store(ctrl, Ctrl.INPUT_WRITE, 8);
    expect(drainRing(ctrl, ring, out)).toEqual({ count: 16, excess: 0 });
    expect(ids(out, 16)).toEqual(Array.from({ length: 16 }, (_, i) => 500 + i));
    expect(Atomics.load(ctrl, Ctrl.INPUT_READ)).toBe(8);
  });

  test('запись ушла вперёд на 2^31: читается не больше ёмкости, лишнее считается', () => {
    const { ctrl, ring } = sharedParts();
    const out = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
    const read = 12345;
    const write = (read + 2 ** 31) | 0; // отрицательное int32
    for (let i = 0; i < RING_CAPACITY; i++) put(ring, write - RING_CAPACITY + i, i);
    Atomics.store(ctrl, Ctrl.INPUT_READ, read);
    Atomics.store(ctrl, Ctrl.INPUT_WRITE, write);
    const r = drainRing(ctrl, ring, out);
    expect(r.count).toBe(RING_CAPACITY);
    expect(r.excess).toBe(2 ** 31 - RING_CAPACITY);
    // Последние ёмкость-записей, оканчивающиеся на счётчике записи, — по порядку.
    expect(ids(out, RING_CAPACITY)).toEqual(Array.from({ length: RING_CAPACITY }, (_, i) => i));
    expect(Atomics.load(ctrl, Ctrl.INPUT_READ)).toBe(write);
    expect(Atomics.load(ctrl, Ctrl.INPUT_DROPPED)).toBe(2 ** 31 - RING_CAPACITY);
  });

  test('запись ушла вперёд через границу 2^32, ровно на ёмкость и на единицу больше', () => {
    const { ctrl, ring } = sharedParts();
    const out = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
    Atomics.store(ctrl, Ctrl.INPUT_READ, -1);
    Atomics.store(ctrl, Ctrl.INPUT_WRITE, RING_CAPACITY - 1);
    expect(drainRing(ctrl, ring, out)).toEqual({ count: RING_CAPACITY, excess: 0 });
    Atomics.store(ctrl, Ctrl.INPUT_READ, -1);
    Atomics.store(ctrl, Ctrl.INPUT_WRITE, RING_CAPACITY);
    expect(drainRing(ctrl, ring, out)).toEqual({ count: RING_CAPACITY, excess: 1 });
    expect(Atomics.load(ctrl, Ctrl.INPUT_DROPPED)).toBe(1);
  });

  test('основной поток: полное кольцо — запись в очередь, порядок сохраняется', () => {
    const posted: ToWorker[] = [];
    const t = new SabTransport({ postMessage: (m: ToWorker) => posted.push(m) } as unknown as Worker);
    const parts = { ctrl: new Int32Array(t.sab, 0, 16), ring: new Uint8Array(t.sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE) };
    const total = RING_CAPACITY + 10;
    for (let i = 0; i < total; i++) enc.resize(t.sink, 100 + i, 50, 1);
    expect(Atomics.load(parts.ctrl, Ctrl.INPUT_WRITE)).toBe(RING_CAPACITY);
    const out = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
    const first = drainRing(parts.ctrl, parts.ring, out);
    expect(first.count).toBe(RING_CAPACITY);
    t.tick(); // очередь уходит в освободившееся кольцо
    const second = drainRing(parts.ctrl, parts.ring, out);
    expect(second.count).toBe(10);
    const widths = Array.from({ length: 10 }, (_, i) => new DataView(out.buffer).getFloat64(i * RECORD_SIZE + 8, true));
    expect(widths).toEqual(Array.from({ length: 10 }, (_, i) => 100 + RING_CAPACITY + i));
    expect(t.dropped()).toBe(0);
  });
});

test.describe('живое состояние (seqlock)', () => {
  test('писатель: версия нечётна, пока пишет слоты, и чётна после', () => {
    const { ctrl, live } = sharedParts();
    const seen: number[] = [];
    const spy = new Proxy(live, {
      get(target, prop) {
        if (prop === 'set') {
          return (values: ArrayLike<number>) => {
            seen.push(Atomics.load(ctrl, Ctrl.LIVE_SEQ));
            target.set(values);
          };
        }
        const v = Reflect.get(target, prop, target);
        return typeof v === 'function' ? v.bind(target) : v;
      },
    });
    writeLive(ctrl, spy, [1, 2, 3]);
    expect(seen.map((s) => s & 1)).toEqual([1]);
    expect(Atomics.load(ctrl, Ctrl.LIVE_SEQ) & 1).toBe(0);
    expect(Array.from(live.subarray(0, 3))).toEqual([1, 2, 3]);
    writeLive(ctrl, live, [4, 5, 6]);
    expect(Atomics.load(ctrl, Ctrl.LIVE_SEQ)).toBe(4);
  });

  test('читатель: до первой записи — null, затем значения', () => {
    const { ctrl, live } = sharedParts();
    const reader = new LiveReader(ctrl, live);
    expect(reader.read()).toBeNull();
    writeLive(ctrl, live, [7, 8]);
    expect(Array.from(reader.read()!.subarray(0, 2))).toEqual([7, 8]);
  });

  test('читатель: нечётная версия — прошлые значения, чтение не зацикливается', () => {
    const { ctrl, live } = sharedParts();
    const reader = new LiveReader(ctrl, live);
    writeLive(ctrl, live, [1]);
    const before = reader.read()!;
    Atomics.add(ctrl, Ctrl.LIVE_SEQ, 1); // писатель «завис» посередине записи
    live[0] = 99;
    expect(reader.read()).toBe(before);
    expect(before[0]).toBe(1);
  });

  test('читатель: запись посреди чтения — повтор; постоянные помехи — прошлые значения', () => {
    const { ctrl, live } = sharedParts();
    const reader = new LiveReader(ctrl, live);
    writeLive(ctrl, live, [1]);
    const before = reader.read()!;
    writeLive(ctrl, live, [2]); // версия сменилась — читатель обязан копировать заново
    let slices = 0;
    const realSlice = live.slice.bind(live);
    // Каждое чтение «рвётся»: пока копируем, писатель успевает сменить версию.
    live.slice = () => {
      slices++;
      const copy = realSlice();
      Atomics.add(ctrl, Ctrl.LIVE_SEQ, 2);
      return copy;
    };
    expect(reader.read()).toBe(before);
    expect(slices).toBe(LIVE_READ_RETRIES);
    // Один обрыв, потом тишина: вторая попытка успешна.
    slices = 0;
    writeLive(ctrl, live, [5]);
    live.slice = () => {
      slices++;
      const copy = realSlice();
      if (slices === 1) Atomics.add(ctrl, Ctrl.LIVE_SEQ, 2);
      return copy;
    };
    const next = reader.read()!;
    expect(slices).toBe(2);
    expect(next[0]).toBe(5);
  });
});

test.describe('предел размера команд', () => {
  const big = (bytes: number): Uint8Array => {
    const a = new Uint8Array(1);
    Object.defineProperty(a, 'byteLength', { value: bytes }); // без выделения памяти
    return a;
  };

  test('Open, Paste и PlaceImage больше предела отклоняются по-русски', () => {
    for (const type of ['open', 'paste', 'placeImage'] as const) {
      const err = commandSizeError({ type, bytes: big(MAX_COMMAND_BYTES[type] + 1) });
      expect(err, type).toMatch(/^Слишком большой объём данных: \d+ МБ, предел для этой команды — \d+ МБ\.$/);
      expect(commandSizeError({ type, bytes: big(MAX_COMMAND_BYTES[type]) }), type).toBeNull();
    }
    expect(commandSizeError({ type: 'save' })).toBeNull();
    expect(commandSizeError(JSON.parse('{"type":"open","bytes":"текст"}'))).toMatch(/^Неверная команда/);
  });

  test('транспорт не отправляет такую команду и возвращает текст ошибки', () => {
    for (const make of [(w: Worker) => new MessageTransport(w), (w: Worker) => new SabTransport(w)]) {
      const posted: unknown[] = [];
      const t = make({ postMessage: (m: unknown) => posted.push(m) } as unknown as Worker);
      const err = t.post({ t: 'cmd', cmd: { type: 'paste', bytes: big(MAX_COMMAND_BYTES.paste + 1) } });
      expect(err).toContain('Слишком большой объём данных');
      expect(posted).toEqual([]);
      expect(t.post({ t: 'cmd', cmd: { type: 'paste', bytes: new Uint8Array(8) } })).toBeNull();
      expect(posted).toHaveLength(1);
    }
  });
});

test.describe('запасной транспорт postMessage', () => {
  function fake() {
    const posted: { msg: ToWorker; transfer?: Transferable[] }[] = [];
    const worker = {
      onmessage: null as ((e: MessageEvent<FromWorker>) => void) | null,
      postMessage: (msg: ToWorker, transfer?: Transferable[]) => posted.push({ msg, transfer }),
    };
    return { posted, worker };
  }

  test('ввод и размер уходят в сообщении кадра; новый кадр ждёт ответа с живым состоянием', () => {
    const { posted, worker } = fake();
    const t = new MessageTransport(worker as unknown as Worker);
    enc.resize(t.sink, 900, 500, 2);
    enc.simple(t.sink, Kind.BLUR);
    t.tick();
    expect(posted).toHaveLength(1);
    const first = posted[0].msg;
    if (first.t !== 'tick' || !first.input) throw new Error('кадр без ввода');
    expect(first.input.byteLength).toBe(2 * RECORD_SIZE);
    const view = new DataView(first.input);
    expect([view.getUint8(0), view.getFloat64(8, true), view.getFloat64(16, true), view.getFloat64(24, true)]).toEqual([
      Kind.RESIZE,
      900,
      500,
      2,
    ]);
    expect(view.getUint8(RECORD_SIZE)).toBe(Kind.BLUR);
    expect(posted[0].transfer).toEqual([first.input]);
    // Ответа нет: следующий кадр не шлётся, ввод копится.
    enc.simple(t.sink, Kind.POINTER_CANCEL);
    t.tick();
    expect(posted).toHaveLength(1);
    // Ответ пришёл: живое состояние читается, ввод уходит.
    const live = new Float64Array(LIVE_SLOTS);
    live[0] = 3;
    worker.onmessage!({ data: { t: 'live', live } } as MessageEvent<FromWorker>);
    expect(t.live()?.[0]).toBe(3);
    t.tick();
    expect(posted).toHaveLength(2);
    const second = posted[1].msg;
    expect(second.t === 'tick' && second.input?.byteLength).toBe(RECORD_SIZE);
  });
});

test.describe('записи ввода совпадают с Rust', () => {
  type Mods = { shiftKey: boolean; altKey: boolean; ctrlKey: boolean; metaKey: boolean };
  const mods: Mods = { shiftKey: true, altKey: false, ctrlKey: true, metaKey: true };
  const pointer = (button: number): PointerEvent => ({ button, timeStamp: 1234.5, ...mods }) as PointerEvent;
  const keyEvent = (code: string, key: string, over: object = {}): KeyboardEvent =>
    ({
      code,
      key,
      timeStamp: 0, // Rust-запись клавиши времени не хранит
      repeat: false,
      shiftKey: false,
      altKey: false,
      ctrlKey: false,
      metaKey: false,
      getModifierState: () => false,
      ...over,
    }) as unknown as KeyboardEvent;

  /** Записи в том же порядке, что `golden_records()` в crates/ferrum-wasm/tests/input.rs. */
  const cases: [string, (sink: enc.Sink) => void][] = [
    ['pointerdown', (s) => enc.pointer(s, Kind.POINTER_DOWN, pointer(0), 10.25, -3.5)],
    ['pointermove', (s) => enc.pointer(s, Kind.POINTER_MOVE, pointer(-1), 1e7, 2e-3)],
    ['pointerup', (s) => enc.pointer(s, Kind.POINTER_UP, pointer(2), 0, 0)],
    ['pointerdown средняя', (s) => enc.pointer(s, Kind.POINTER_DOWN, pointer(1), 5, 5)],
    ['pointercancel', (s) => enc.simple(s, Kind.POINTER_CANCEL)],
    [
      'wheel',
      (s) =>
        enc.wheel(s, { deltaMode: 0, deltaX: -4, deltaY: 120, shiftKey: false, altKey: false, ctrlKey: true, metaKey: false } as WheelEvent, 100, 200, 720),
    ],
    ['pinch', (s) => enc.pinch(s, 3, 4, 1.25)],
    [
      'keydown',
      (s) =>
        enc.key(
          s,
          Kind.KEY_DOWN,
          keyEvent('BracketLeft', 'х', { altKey: true, repeat: true, getModifierState: (m: string) => m === 'AltGraph' }),
        ),
    ],
    ['keyup', (s) => enc.key(s, Kind.KEY_UP, keyEvent('BracketLeft', 'х', { altKey: true, getModifierState: () => false }))],
    ['blur', (s) => enc.simple(s, Kind.BLUR)],
    ['resize', (s) => enc.resize(s, 1280, 720, 2)],
    ['длинная клавиша', (s) => enc.key(s, Kind.KEY_DOWN, keyEvent('LaunchApplication1', 'ы'.repeat(14)))],
    ['обрезка посреди символа', (s) => enc.key(s, Kind.KEY_DOWN, keyEvent('KeyA', 'a' + '\u{1F600}'.repeat(6)))],
  ];

  test('те же байты, что `encode` в Rust', () => {
    const golden = readFileSync(new URL('../../../crates/ferrum-wasm/tests/golden/input_records.hex', import.meta.url), 'utf8')
      .trim()
      .split('\n');
    expect(golden).toHaveLength(cases.length);
    cases.forEach(([name, make], i) => {
      // Буфер заранее забит единицами: кодировщик обязан обнулить неиспользуемые поля.
      const buf = new Uint8Array(RECORD_SIZE).fill(0xff);
      make((fill) => fill(new DataView(buf.buffer), 0));
      const hex = Array.from(buf, (b) => b.toString(16).padStart(2, '0')).join('');
      expect(hex, name).toBe(golden[i]);
    });
  });
});
