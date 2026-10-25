// Поле числа (NumberField дизайн-системы): подпись, которую можно тянуть мышью
// (шаг за пиксель, Shift ×10, Alt ×0.1), ввод выражений (`120+16`, `50%`),
// Enter применяет, Esc откатывает, стрелки — шаг 1 (Shift — 10). «Смешанные» —
// у выделенных разные значения. Перетаскивание подписи — жест: превью, затем
// один шаг отмены при отпускании; набранное значение — тоже один шаг.

import { createEffect, createSignal, type JSX } from 'solid-js';
import type { EditPhase } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import {
  arrowStep,
  clamp,
  fieldKey,
  formatNumber,
  parseFieldInput,
  scrubValue,
  type FieldInput,
} from './number';

export interface NumberFieldProps {
  /** Подпись (1–2 символа) или иконка. */
  label: JSX.Element;
  /** Название поля для чтения с экрана («Ширина»). */
  name: string;
  value: number;
  mixed?: boolean;
  /** Отображение: значение поля → текст (проценты, градусы). */
  scale?: number;
  suffix?: string;
  decimals?: number;
  step?: number;
  min?: number;
  max?: number;
  /** От чего считать «%» при вводе (в единицах поля). */
  percentOf?: number;
  disabled?: boolean;
  /** Правка: значение в единицах поля (`set`) или прибавка к каждому (`add`). */
  onEdit: (input: FieldInput, phase: EditPhase) => void;
  class?: string;
}

export function NumberField(props: NumberFieldProps) {
  const ed = useEditor();
  const scale = () => props.scale ?? 1;
  const shown = () => (props.mixed ? 'Смешанные' : `${formatNumber(props.value * scale(), props.decimals ?? 2)}${props.suffix ?? ''}`);
  const [focused, setFocused] = createSignal(false);
  // Пользователь набирает: текст поля его, значение из сводки его не затирает.
  const [dirty, setDirty] = createSignal(false);
  let input!: HTMLInputElement;
  createEffect(() => {
    const text = focused() && props.mixed ? '' : shown();
    if (dirty()) return;
    input.value = text;
    // Значение пришло, пока поле в фокусе (стрелка, живая правка): выделение
    // сбросилось бы, и следующая цифра дописалась бы к старому числу.
    if (focused() && !props.mixed && document.activeElement === input) input.select();
  });
  let scrub: { x: number; start: number; moved: boolean } | null = null;

  const toField = (v: number) => clamp(v, props.min, props.max) / scale();
  const emit = (inp: FieldInput, phase: EditPhase) => {
    if (inp.kind === 'set') props.onEdit({ kind: 'set', value: toField(inp.value) }, phase);
    else props.onEdit({ kind: 'add', delta: inp.delta / scale() }, phase);
  };

  // Правка без изменений шага не создаёт: то же значение (после ограничения) — не шлём.
  const unchanged = (inp: FieldInput) =>
    inp.kind === 'set' ? !props.mixed && toField(inp.value) === props.value : inp.delta === 0;
  const apply = (inp: FieldInput) => {
    if (!unchanged(inp)) emit(inp, 'commit');
  };

  const commitText = () => {
    if (!dirty()) return;
    // Текст — до снятия флага: эффект выше сразу вернёт в поле значение сводки.
    const text = input.value.replace(props.suffix ?? '\u0000', '');
    setDirty(false);
    const parsed = parseFieldInput(text, {
      current: props.value * scale(),
      mixed: props.mixed,
      percentOf: props.percentOf,
    });
    // Не число или то же значение — без правки, поле показывает прежнее.
    if (parsed) apply(parsed);
    input.value = focused() && props.mixed ? '' : shown();
  };

  // Esc во время перетаскивания подписи — отмена жеста без следа в истории.
  const onEscape = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || !scrub) return;
    e.preventDefault();
    e.stopPropagation();
    if (scrub.moved) props.onEdit({ kind: 'add', delta: 0 }, 'cancel');
    scrub = null;
  };
  const endScrub = () => window.removeEventListener('keydown', onEscape, true);

  const onPointerDown = (e: PointerEvent) => {
    if (props.disabled || e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    scrub = { x: e.clientX, start: props.mixed ? 0 : props.value * scale(), moved: false };
    window.addEventListener('keydown', onEscape, true);
  };
  const onPointerMove = ed.core.costed((e: PointerEvent) => {
    if (!scrub) return;
    const dx = e.clientX - scrub.x;
    if (!scrub.moved && Math.abs(dx) < 1) return;
    scrub.moved = true;
    const v = scrubValue(scrub.start, dx, props.step ?? 1, { shift: e.shiftKey, alt: e.altKey });
    // У «Смешанных» перетаскивание прибавляет к каждому своё.
    emit(props.mixed ? { kind: 'add', delta: v } : { kind: 'set', value: v }, 'preview');
  });
  const onPointerUp = (e: PointerEvent) => {
    endScrub();
    const s = scrub;
    scrub = null;
    if (!s?.moved) return;
    const v = scrubValue(s.start, e.clientX - s.x, props.step ?? 1, { shift: e.shiftKey, alt: e.altKey });
    const inp: FieldInput = props.mixed ? { kind: 'add', delta: v } : { kind: 'set', value: v };
    // Вернули на исходное значение — жест отменяется (превью уже менял документ).
    if (unchanged(inp)) props.onEdit({ kind: 'add', delta: 0 }, 'cancel');
    else emit(inp, 'commit');
  };

  return (
    <label class={`fe-field fe-num ${props.class ?? ''}`} classList={{ 'fe-field--disabled': props.disabled }}>
      <span
        class="fe-field__label"
        aria-hidden="true"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          endScrub();
          if (scrub?.moved) props.onEdit({ kind: 'add', delta: 0 }, 'cancel');
          scrub = null;
        }}
      >
        {props.label}
      </span>
      <input
        ref={input}
        aria-label={props.name}
        classList={{ 'fe-num--mixed': props.mixed && !focused() }}
        placeholder={props.mixed ? 'Смешанные' : undefined}
        disabled={props.disabled}
        inputMode="decimal"
        spellcheck={false}
        onFocus={() => {
          setFocused(true);
          queueMicrotask(() => input.select());
        }}
        onInput={() => setDirty(true)}
        onBlur={() => {
          commitText();
          setFocused(false);
        }}
        onKeyDown={(e) => {
          const action = fieldKey(e.key);
          if (action === 'commit') {
            commitText();
            input.select();
          } else if (action === 'revert') {
            setDirty(false);
            input.blur();
          } else {
            const d = arrowStep(e.key, e.shiftKey);
            if (!d) return;
            // Каждое нажатие — свой шаг отмены (без склейки). Набранный, но не
            // применённый текст — база шага, как в Figma.
            let base = props.value * scale();
            if (dirty()) {
              const typed = parseFieldInput(input.value.replace(props.suffix ?? '\u0000', ''), {
                current: base,
                mixed: props.mixed,
                percentOf: props.percentOf,
              });
              setDirty(false);
              if (typed?.kind === 'set') base = typed.value;
            }
            apply(props.mixed ? { kind: 'add', delta: d } : { kind: 'set', value: base + d });
          }
          e.preventDefault();
        }}
      />
    </label>
  );
}
