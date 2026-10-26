// Записи кольцевого буфера ввода — те же байты, что разбирает
// crates/ferrum-wasm/src/input.rs. Кодирование — O(1) на событие: основной поток
// только перекладывает поля события в буфер, вся обработка — в worker'е.

import { Field, Flag, Kind, RECORD_SIZE } from './layout.gen';

/** Куда положить запись: представление буфера и смещение начала записи. */
export type Sink = (fill: (view: DataView, at: number) => void) => void;

const encoder = new TextEncoder();
const scratch = new Uint8Array(Field.TEXT_MAX * 4);

function flags(e: { shiftKey: boolean; altKey: boolean; ctrlKey: boolean; metaKey: boolean }): number {
  return (
    (e.shiftKey ? Flag.SHIFT : 0) |
    (e.altKey ? Flag.ALT : 0) |
    (e.ctrlKey ? Flag.CTRL : 0) |
    (e.metaKey ? Flag.META : 0)
  );
}

function clear(view: DataView, at: number): void {
  for (let i = 0; i < RECORD_SIZE; i += 8) view.setFloat64(at + i, 0, true);
}

/** UTF-8 строки не длиннее TEXT_MAX байт, обрезанной по границе символа. */
function putText(view: DataView, at: number, lenAt: number, textAt: number, text: string): void {
  const { written } = encoder.encodeInto(text, scratch);
  let len = Math.min(written, Field.TEXT_MAX);
  // Продолжение символа UTF-8 — 0b10xxxxxx: отступаем к его началу.
  while (len < written && len > 0 && (scratch[len] & 0xc0) === 0x80) len--;
  view.setUint8(at + lenAt, len);
  for (let i = 0; i < len; i++) view.setUint8(at + textAt + i, scratch[i]);
}

export type PointerKind = typeof Kind.POINTER_DOWN | typeof Kind.POINTER_MOVE | typeof Kind.POINTER_UP;

export function pointer(sink: Sink, kind: PointerKind, e: PointerEvent, x: number, y: number): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, kind);
    // У движения `button` = −1: кнопку редактор берёт из нажатия. Нажатия и
    // отпускания других кнопок (3, 4) сюда не доходят — их отсекает `Core`.
    v.setUint8(at + Field.BUTTON, e.button >= 0 && e.button <= 2 ? e.button : 0);
    v.setUint8(at + Field.FLAGS, flags(e));
    v.setFloat64(at + Field.A, x, true);
    v.setFloat64(at + Field.B, y, true);
    v.setFloat64(at + Field.C, e.timeStamp, true);
  });
}

export function simple(sink: Sink, kind: typeof Kind.POINTER_CANCEL | typeof Kind.BLUR): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, kind);
  });
}

/** Прокрутка в CSS-пикселях: строки — по 16 px, страницы — высота холста. */
export function wheel(sink: Sink, e: WheelEvent, x: number, y: number, pageHeight: number): void {
  const k = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? pageHeight : 1;
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, Kind.WHEEL);
    v.setUint8(at + Field.FLAGS, flags(e));
    v.setFloat64(at + Field.A, x, true);
    v.setFloat64(at + Field.B, y, true);
    v.setFloat64(at + Field.C, e.deltaX * k, true);
    v.setFloat64(at + Field.D, e.deltaY * k, true);
  });
}

export function pinch(sink: Sink, x: number, y: number, scale: number): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, Kind.PINCH);
    v.setFloat64(at + Field.A, x, true);
    v.setFloat64(at + Field.B, y, true);
    v.setFloat64(at + Field.C, scale, true);
  });
}

export function key(sink: Sink, kind: typeof Kind.KEY_DOWN | typeof Kind.KEY_UP, e: KeyboardEvent): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, kind);
    // AltGr — только настоящее состояние клавиши, не вывод из Ctrl+Alt
    // (договор с редактором, см. `KeyEvent::alt_graph`).
    const altGraph = e.getModifierState('AltGraph') ? Flag.ALT_GRAPH : 0;
    v.setUint8(at + Field.FLAGS, flags(e) | altGraph | (e.repeat ? Flag.REPEAT : 0));
    v.setFloat64(at + Field.A, e.timeStamp, true);
    putText(v, at, Field.CODE_LEN, Field.CODE, e.code);
    putText(v, at, Field.KEY_LEN, Field.KEY, e.key);
  });
}

export function resize(sink: Sink, width: number, height: number, dpr: number): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, Kind.RESIZE);
    v.setFloat64(at + Field.A, width, true);
    v.setFloat64(at + Field.B, height, true);
    v.setFloat64(at + Field.C, dpr, true);
  });
}

/** Физическая клавиша Alt (Option на macOS). */
export function isAltCode(code: string): boolean {
  return code === 'AltLeft' || code === 'AltRight';
}

/** Одиночное нажатие Alt: между нажатием и отпусканием не было других клавиш. На
 * Windows отпускание такого Alt переводит фокус в меню браузера — пока Alt держат
 * для измерений на холсте, этого быть не должно. Указатель «одиночность» не
 * снимает: отпускание после Alt+перетаскивания так же уводило бы фокус. */
export function createLoneAlt(): { down(e: KeyboardEvent): void; up(e: KeyboardEvent): boolean; reset(): void } {
  let alone = false;
  return {
    down(e) {
      if (!isAltCode(e.code)) alone = false;
      else if (!e.repeat) alone = true;
    },
    up(e) {
      if (!isAltCode(e.code)) return false;
      const lone = alone;
      alone = false;
      return lone;
    },
    reset() {
      alone = false;
    },
  };
}
