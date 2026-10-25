// Палитра цвета: область HSB (насыщенность × яркость), полосы тона и
// прозрачности, HEX, прозрачность в процентах, пипетка (`EyeDropper`, где он
// есть; иначе кнопки нет).
//
// Движение внутри палитры — жест: превью не чаще кадра, один шаг отмены при
// отпускании (цвет не изменился — шага нет, а ушедшее превью отменяется). Правка
// адресована тем, кого выделяли в начале жеста (`begin`): выделение может
// смениться посреди жеста. Esc, потеря указателя или закрытие палитры во время
// жеста — цвет на начало жеста, без шага. Палитра держит тон сама: у серого
// (насыщенность 0) тон не теряется.

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import type { EditPhase } from '../wasm/ferrum_wasm';
import { cssColor, hexHasAlpha, hsbToRgb, parseHex, parseOpacityText, rgbToHsb, toHex6, type Hsb, type Rgb } from './colour';
import { CommitInput, type Capture } from './commit-input';
import { useEditor } from './context';
import { Icon } from './icons';
import { clamp, formatNumber } from './number';

export interface PickerProps<C extends Capture> {
  title: string;
  color: Rgb;
  /** 0–1. */
  alpha: number;
  anchor: HTMLElement;
  /** Снимок на начало правки (жеста, набора, стрелки): адресат и то, от чего строится правка. */
  begin: () => C;
  onChange: (color: Rgb, alpha: number, phase: EditPhase, base: C) => void;
  onClose: () => void;
}

type Area = 'sb' | 'hue' | 'alpha';

interface EyeDropperApi {
  open(): Promise<{ sRGBHex: string }>;
}

export function hasEyeDropper(): boolean {
  return typeof (window as unknown as { EyeDropper?: unknown }).EyeDropper === 'function';
}

export function ColorPicker<C extends Capture>(props: PickerProps<C>) {
  const ed = useEditor();
  // Свотч — один раз: после закрытия палитры родитель убирает его из props.
  const anchor = props.anchor;
  const [hsb, setHsb] = createSignal<Hsb>(rgbToHsb(props.color));
  const [alpha, setAlpha] = createSignal(props.alpha);
  let el!: HTMLDivElement;
  let disposed = false;
  let raf = 0;
  let drag: {
    area: Area;
    target: HTMLElement;
    rect: DOMRect | null;
    start: { hsb: Hsb; alpha: number; key: string };
    base: C;
    /** Превью уходило в ядро: при отмене нужен `cancel`. */
    sent: boolean;
    last: string;
  } | null = null;

  const rgb = () => hsbToRgb(hsb());
  /** Что реально уйдёт в документ (цвет по байтам и процент прозрачности). */
  const keyOf = (c: Rgb = rgb(), a = alpha()) => `${toHex6(c)}/${Math.round(a * 100)}`;
  const emit = (phase: EditPhase, base: C, c: Rgb = rgb(), a = alpha()) => props.onChange(c, a, phase, base);

  const apply = (area: Area, rect: DOMRect, x: number, y: number) => {
    const fx = clamp((x - rect.left) / rect.width, 0, 1);
    const fy = clamp((y - rect.top) / rect.height, 0, 1);
    if (area === 'sb') setHsb({ h: hsb().h, s: fx, b: 1 - fy });
    else if (area === 'hue') setHsb({ ...hsb(), h: fx * 360 });
    else setAlpha(Math.round(fx * 100) / 100);
  };

  // Превью не чаще кадра и только с новым значением.
  const flush = () => {
    raf = 0;
    const d = drag;
    if (!d) return;
    const key = keyOf();
    if (key === d.last) return;
    d.last = key;
    d.sent = true;
    emit('preview', d.base);
  };
  const schedule = () => {
    if (!raf) raf = requestAnimationFrame(flush);
  };
  const dropPreview = () => {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
  };

  /** Жест кончился записью: изменился — один шаг, нет — превью (если было) отменяется. */
  const finish = (d: NonNullable<typeof drag>) => {
    if (keyOf() !== d.start.key) emit('commit', d.base);
    else if (d.sent) emit('cancel', d.base, rgbOf(d.start.hsb), d.start.alpha);
  };
  const rgbOf = (h: Hsb) => hsbToRgb(h);

  const down = (area: Area) => (e: PointerEvent) => {
    if (e.button !== 0 || drag) return;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    target.focus();
    const at = { x: e.clientX, y: e.clientY };
    const k0 = keyOf();
    drag = { area, target, rect: null, start: { hsb: hsb(), alpha: alpha(), key: k0 }, base: props.begin(), sent: false, last: k0 };
    ed.scheduler.measure(() => {
      if (!drag) return;
      drag.rect = target.getBoundingClientRect();
      const rect = drag.rect;
      ed.scheduler.mutate(() => {
        if (!drag) return;
        apply(area, rect, at.x, at.y);
        schedule();
      });
    });
  };
  const move = ed.core.costed((e: PointerEvent) => {
    if (!drag?.rect) return;
    apply(drag.area, drag.rect, e.clientX, e.clientY);
    schedule();
  });
  const up = (e: PointerEvent) => {
    const d = drag;
    drag = null;
    dropPreview();
    if (!d) return;
    if (d.rect) {
      finish(d);
      return;
    }
    // Отпустили раньше первого замера (быстрый щелчок): значение — по точке
    // отпускания, и сразу запись.
    const at = { x: e.clientX, y: e.clientY };
    ed.scheduler.measure(() => {
      const rect = d.target.getBoundingClientRect();
      ed.scheduler.mutate(() => {
        if (disposed) return;
        apply(d.area, rect, at.x, at.y);
        finish(d);
      });
    });
  };

  /** Жест прерван (Esc, потеря указателя, закрытие палитры): цвет на начало жеста. */
  const cancelDrag = () => {
    const d = drag;
    drag = null;
    dropPreview();
    if (!d) return;
    if (!disposed) {
      setHsb(d.start.hsb);
      setAlpha(d.start.alpha);
    }
    if (d.sent) emit('cancel', d.base, rgbOf(d.start.hsb), d.start.alpha);
  };
  onCleanup(() => {
    cancelDrag();
    disposed = true;
  });

  /** Стрелки на области и полосах: шаг 1 % (Shift — 10 %), тон — 1° (10°). Каждое
   * нажатие — свой шаг отмены. */
  const key = (area: Area) => (e: KeyboardEvent) => {
    const k = e.shiftKey ? 10 : 1;
    const dx = e.key === 'ArrowRight' ? k : e.key === 'ArrowLeft' ? -k : 0;
    const dy = e.key === 'ArrowUp' ? k : e.key === 'ArrowDown' ? -k : 0;
    if (!dx && !dy) return;
    e.preventDefault();
    if (drag) return;
    const before = keyOf();
    const c = hsb();
    if (area === 'sb') setHsb({ h: c.h, s: clamp(c.s + dx / 100, 0, 1), b: clamp(c.b + dy / 100, 0, 1) });
    else if (area === 'hue') setHsb({ ...c, h: (((c.h + dx + dy) % 360) + 360) % 360 });
    else setAlpha(clamp(Math.round(alpha() * 100 + dx + dy) / 100, 0, 1));
    if (keyOf() !== before) emit('commit', props.begin());
  };

  /** Закрыть и вернуть фокус на свотч. */
  const close = () => {
    props.onClose();
    anchor.focus?.();
  };

  onMount(() => {
    ed.scheduler.measure(() => {
      const r = anchor.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      const x = Math.max(8, Math.min(r.left - box.width - 8, innerWidth - box.width - 8));
      const y = Math.max(8, Math.min(r.top - 40, innerHeight - box.height - 8));
      ed.scheduler.mutate(() => {
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        el.style.visibility = 'visible';
        el.querySelector<HTMLElement>('.fe-picker__sb')?.focus();
      });
    });
    const outside = (e: PointerEvent) => {
      if (!el.contains(e.target as Node) && !anchor.contains(e.target as Node)) props.onClose();
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      // Esc во время жеста: цвет на начало жеста, без шага истории.
      if (drag) cancelDrag();
      else close();
    };
    // Фокус ушёл из палитры (Tab) — палитра закрывается.
    const leave = (e: FocusEvent) => {
      const t = e.target as Node;
      if (!el.contains(t) && !anchor.contains(t)) props.onClose();
    };
    const t = setTimeout(() => document.addEventListener('pointerdown', outside, true));
    window.addEventListener('keydown', esc, true);
    document.addEventListener('focusin', leave);
    onCleanup(() => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('keydown', esc, true);
      document.removeEventListener('focusin', leave);
    });
  });

  const pick = async () => {
    const Ctor = (window as unknown as { EyeDropper: new () => EyeDropperApi }).EyeDropper;
    const base = props.begin();
    try {
      const res = await new Ctor().open();
      const c = parseHex(res.sRGBHex);
      // Палитру закрыли, пока пипетка была открыта: выделение могло смениться.
      if (!c || disposed || toHex6(c) === toHex6(rgb())) return;
      setHsb(rgbToHsb(c));
      emit('commit', base);
    } catch (e) {
      // Пользователь отменил пипетку (Esc) — ничего не меняем; любой другой сбой — одно уведомление.
      if (!(e instanceof DOMException && e.name === 'AbortError')) {
        ed.store.toast('Не удалось взять цвет пипеткой', 'error');
      }
    }
  };

  // HEX и % применяются по Enter и уходу фокуса — только если значение изменилось:
  // Enter и затем уход фокуса — одна правка, а не две.
  const hexInput = (text: string, base: C) => {
    const c = parseHex(text);
    if (!c) return;
    const a = hexHasAlpha(text) ? c.a / 255 : alpha();
    if (toHex6(c) === toHex6(rgb()) && Math.round(a * 100) === Math.round(alpha() * 100)) return;
    setHsb(rgbToHsb(c));
    setAlpha(Math.round(a * 100) / 100);
    emit('commit', base);
  };

  const opacityInput = (text: string, base: C) => {
    const a = parseOpacityText(text, alpha() * 100);
    if (a === null || Math.round(a * 100) === Math.round(alpha() * 100)) return;
    setAlpha(a);
    emit('commit', base);
  };

  const pure = () => cssColor(hsbToRgb({ h: hsb().h, s: 1, b: 1 }));

  return (
    <Portal>
      <div ref={el} class="fe-picker" role="dialog" aria-label={props.title} style={{ visibility: 'hidden' }}>
        <div class="fe-picker__head">
          <span class="t-strong">{props.title}</span>
          <button class="fe-tool fe-tool--small" aria-label="Закрыть палитру" data-tip="Закрыть палитру" data-tip-key="Esc" onClick={close}>
            <Icon name="close" />
          </button>
        </div>
        <div
          class="fe-picker__sb"
          role="slider"
          tabindex="0"
          aria-label="Насыщенность и яркость"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(hsb().s * 100)}
          aria-valuetext={`насыщенность ${Math.round(hsb().s * 100)}%, яркость ${Math.round(hsb().b * 100)}%`}
          style={{ background: pure() }}
          onPointerDown={down('sb')}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={cancelDrag}
          onLostPointerCapture={cancelDrag}
          onKeyDown={key('sb')}
        >
          <span
            class="fe-picker__thumb"
            style={{ left: `${hsb().s * 100}%`, top: `${(1 - hsb().b) * 100}%`, background: cssColor(rgb()) }}
          />
        </div>
        <div class="fe-picker__sliders">
          <Show when={hasEyeDropper()}>
            <button class="fe-tool fe-tool--small" aria-label="Пипетка" data-tip="Пипетка" onClick={pick}>
              <Icon name="pipette" />
            </button>
          </Show>
          <div class="fe-picker__bars">
            <div
              class="fe-picker__bar fe-picker__hue"
              role="slider"
              tabindex="0"
              aria-label="Тон"
              aria-valuemin={0}
              aria-valuemax={360}
              aria-valuenow={Math.round(hsb().h)}
              onPointerDown={down('hue')}
              onPointerMove={move}
              onPointerUp={up}
              onPointerCancel={cancelDrag}
              onLostPointerCapture={cancelDrag}
              onKeyDown={key('hue')}
            >
              <span class="fe-picker__knob" style={{ left: `${(hsb().h / 360) * 100}%`, background: pure() }} />
            </div>
            <div
              class="fe-picker__bar fe-picker__alpha"
              role="slider"
              tabindex="0"
              aria-label="Прозрачность"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(alpha() * 100)}
              style={{ '--picker-color': cssColor(rgb()) }}
              onPointerDown={down('alpha')}
              onPointerMove={move}
              onPointerUp={up}
              onPointerCancel={cancelDrag}
              onLostPointerCapture={cancelDrag}
              onKeyDown={key('alpha')}
            >
              <span class="fe-picker__knob" style={{ left: `${alpha() * 100}%`, background: cssColor(rgb(), alpha()) }} />
            </div>
          </div>
        </div>
        <div class="fe-row">
          <CommitInput
            name="HEX"
            class="fe-row__grow"
            prefix={<span class="fe-field__label">#</span>}
            text={toHex6(rgb())}
            maxLength={10}
            capture={props.begin}
            onCommit={hexInput}
          />
          <CommitInput
            name="Прозрачность цвета"
            class="fe-picker__opacity"
            text={`${formatNumber(alpha() * 100, 0)}%`}
            maxLength={16}
            capture={props.begin}
            onCommit={opacityInput}
          />
        </div>
      </div>
    </Portal>
  );
}
