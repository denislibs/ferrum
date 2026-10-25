// Поле числа (NumberField дизайн-системы): подпись, которую можно тянуть мышью
// (шаг за пиксель, Shift ×10, Alt ×0.1), ввод выражений (`120+16`, `50%`),
// Enter применяет, Esc откатывает, стрелки — шаг 1 (Shift — 10). «Смешанные» —
// у выделенных разные значения. Перетаскивание подписи — жест: превью, затем
// один шаг отмены при отпускании; набранное значение — тоже один шаг.

import { createEffect, createSignal, on, onCleanup, untrack, type JSX } from 'solid-js';
import type { EditPhase } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import {
  arrowStep,
  clamp,
  fieldKey,
  formatNumber,
  MAX_INPUT,
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
  /** `ids` — узлы, которые правятся с начала жеста: выделение могло смениться. */
  onEdit: (input: FieldInput, phase: EditPhase, ids: string[]) => void;
  class?: string;
}

export function NumberField(props: NumberFieldProps) {
  const ed = useEditor();
  const scale = () => props.scale ?? 1;
  const decimals = () => props.decimals ?? 2;
  const shown = () => (props.mixed ? 'Смешанные' : `${formatNumber(props.value * scale(), decimals())}${props.suffix ?? ''}`);
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

  // Правка относится к тем, кого выделяли в её начале (как в Figma): выделение
  // успевает смениться до того, как набранное применится. Здесь же — значение поля
  // на тот момент, от которого считаются `*2` и `%`.
  interface Session {
    ids: string[];
    raw: number;
    current: number;
    mixed: boolean;
  }
  let session: Session | null = null;
  const begin = (): Session =>
    (session ??= untrack(() => ({
      ids: [...ed.store.state.selectionIds],
      raw: props.value,
      current: props.value * scale(),
      mixed: !!props.mixed,
    })));

  let scrub: { x: number; start: number; moved: boolean; s: Session } | null = null;
  let pending: FieldInput | null = null;
  let lastPreview = '';
  let raf = 0;

  const toField = (v: number) => clamp(v, props.min, props.max) / scale();
  const emit = (inp: FieldInput, phase: EditPhase, s: Session) => {
    if (inp.kind === 'set') props.onEdit({ kind: 'set', value: toField(inp.value) }, phase, s.ids);
    else props.onEdit({ kind: 'add', delta: inp.delta / scale() }, phase, s.ids);
  };

  // Правка без изменений шага не создаёт: то же значение — прежнее, ограниченное
  // или совпадающее с показанным в поле (30.004 при показанных «30°») — не шлём.
  const unchanged = (inp: FieldInput, s: Session) => {
    if (inp.kind === 'add') return inp.delta === 0;
    if (s.mixed) return false;
    const shownValue = Number(formatNumber(s.raw * scale(), decimals()));
    return toField(inp.value) === s.raw || inp.value === shownValue;
  };
  const apply = (inp: FieldInput, s: Session) => {
    if (!unchanged(inp, s)) emit(inp, 'commit', s);
  };

  const typed = (s: Session) =>
    parseFieldInput(input.value.replace(props.suffix ?? '\u0000', ''), {
      current: s.current,
      mixed: s.mixed,
      percentOf: props.percentOf,
    });

  const commitText = () => {
    if (!dirty()) return;
    // Текст — до снятия флага: эффект выше сразу вернёт в поле значение сводки.
    const s = begin();
    const parsed = typed(s);
    setDirty(false);
    session = null;
    // Не число или то же значение — без правки, поле показывает прежнее.
    if (parsed) apply(parsed, s);
    input.value = focused() && props.mixed ? '' : shown();
  };

  // Выделение сменилось, пока в поле набрано: щелчок в другом месте применяет
  // набранное к прежним узлам.
  createEffect(
    on(
      () => ed.store.state.selectionIds,
      (ids) => {
        if (!dirty() || !session) return;
        const same = ids.length === session.ids.length && ids.every((id, i) => id === session!.ids[i]);
        if (!same) commitText();
      },
      { defer: true },
    ),
  );

  const scrubInput = (v: number, s: Session): FieldInput => (s.mixed ? { kind: 'add', delta: v } : { kind: 'set', value: v });

  // Превью не чаще кадра и только с новым значением.
  const flush = () => {
    raf = 0;
    const inp = pending;
    pending = null;
    if (!inp || !scrub) return;
    const key = JSON.stringify(inp);
    if (key === lastPreview) return;
    lastPreview = key;
    emit(inp, 'preview', scrub.s);
  };
  const dropPreview = () => {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    pending = null;
    lastPreview = '';
  };

  const cancelScrub = () => {
    dropPreview();
    if (scrub?.moved) props.onEdit({ kind: 'add', delta: 0 }, 'cancel', scrub.s.ids);
    scrub = null;
  };
  // Esc во время перетаскивания подписи — отмена жеста без следа в истории.
  const onEscape = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || !scrub) return;
    e.preventDefault();
    e.stopPropagation();
    cancelScrub();
  };
  const endScrub = () => window.removeEventListener('keydown', onEscape, true);
  onCleanup(() => {
    endScrub();
    cancelScrub();
  });

  const onPointerDown = (e: PointerEvent) => {
    if (props.disabled || e.button !== 0) return;
    e.preventDefault();
    // Набранное в этом или другом поле применяется до жеста (preventDefault не
    // даёт фокусу уйти сам).
    const a = document.activeElement;
    if (a instanceof HTMLInputElement && a.closest('.fe-num')) a.blur();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const s = untrack(() => ({
      ids: [...ed.store.state.selectionIds],
      raw: props.value,
      current: props.value * scale(),
      mixed: !!props.mixed,
    }));
    scrub = { x: e.clientX, start: props.mixed ? 0 : props.value * scale(), moved: false, s };
    lastPreview = '';
    window.addEventListener('keydown', onEscape, true);
  };
  const onPointerMove = ed.core.costed((e: PointerEvent) => {
    if (!scrub) return;
    const dx = e.clientX - scrub.x;
    if (!scrub.moved && Math.abs(dx) < 1) return;
    scrub.moved = true;
    // У «Смешанных» перетаскивание прибавляет к каждому своё.
    pending = scrubInput(scrubValue(scrub.start, dx, props.step ?? 1, { shift: e.shiftKey, alt: e.altKey }), scrub.s);
    if (!raf) raf = requestAnimationFrame(flush);
  });
  const onPointerUp = (e: PointerEvent) => {
    endScrub();
    dropPreview();
    const sc = scrub;
    scrub = null;
    if (!sc?.moved) return;
    const inp = scrubInput(scrubValue(sc.start, e.clientX - sc.x, props.step ?? 1, { shift: e.shiftKey, alt: e.altKey }), sc.s);
    // Вернули на исходное значение — жест отменяется (превью уже менял документ).
    if (unchanged(inp, sc.s)) props.onEdit({ kind: 'add', delta: 0 }, 'cancel', sc.s.ids);
    else emit(inp, 'commit', sc.s);
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
          cancelScrub();
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
        maxLength={MAX_INPUT}
        spellcheck={false}
        onFocus={() => {
          setFocused(true);
          queueMicrotask(() => input.select());
        }}
        onInput={() => {
          begin();
          setDirty(true);
        }}
        onBlur={() => {
          commitText();
          setFocused(false);
        }}
        onKeyDown={(e) => {
          // Выбор варианта в IME (Enter, стрелки) — не команда поля.
          if (e.isComposing || e.keyCode === 229) return;
          const action = fieldKey(e.key);
          if (action === 'commit') {
            commitText();
            input.select();
          } else if (action === 'revert') {
            setDirty(false);
            session = null;
            input.blur();
          } else {
            const d = arrowStep(e.key, e.shiftKey);
            if (!d) return;
            // Каждое нажатие — свой шаг отмены (без склейки). Набранный, но не
            // применённый текст — база шага, как в Figma.
            const s = begin();
            let base = props.value * scale();
            if (dirty()) {
              const t = typed(s);
              setDirty(false);
              if (t?.kind === 'set') base = t.value;
            }
            session = null;
            apply(s.mixed ? { kind: 'add', delta: d } : { kind: 'set', value: base + d }, s);
          }
          e.preventDefault();
        }}
      />
    </label>
  );
}
