// Поле переименования на месте — одно для имени файла, страницы и слоя:
// - начальное имя читается один раз: события ядра набранное не переписывают;
// - открылось — фокус и выделение текста;
// - Enter и уход фокуса применяют, Esc закрывает без правки; применяется ровно
//   один раз, а поле, которое убрали, пока в нём набрано (строка ушла из окна,
//   сменилась страница), применяет набранное, а не теряет;
// - после Enter и Esc фокус возвращается туда, откуда начали (`returnFocus`);
// - не длиннее `MAX_NAME_CHARS`; клавиши не уходят ни в строку, ни на холст.
// Изменилось ли имя (и слать ли команду), решает `onCommit`.

import { onCleanup, untrack } from 'solid-js';
import { MAX_NAME_CHARS } from '../core/limits';

export interface RenameInputProps {
  /** Название поля для чтения с экрана. */
  label: string;
  /** Имя на момент открытия. */
  value: string;
  class?: string;
  /** Набранное имя: вызывается не больше раза, после Enter, ухода фокуса или когда поле убрали. */
  onCommit: (name: string) => void;
  /** Поле пора закрыть (после применения или Esc). */
  onClose: () => void;
  /** Куда вернуть фокус после Enter и Esc. */
  returnFocus?: () => HTMLElement | null | undefined;
}

export function RenameInput(props: RenameInputProps) {
  let el!: HTMLInputElement;
  let done = false;
  const finish = (commit: boolean) => {
    if (done) return;
    done = true;
    if (commit) props.onCommit(el.value);
    props.onClose();
  };
  const refocus = () => queueMicrotask(() => props.returnFocus?.()?.focus({ preventScroll: true }));
  onCleanup(() => finish(true));
  queueMicrotask(() => {
    if (done || !el.isConnected) return;
    el.focus();
    el.select();
  });
  return (
    <input
      ref={el}
      class={`fe-rename ${props.class ?? ''}`}
      aria-label={props.label}
      maxlength={MAX_NAME_CHARS}
      spellcheck={false}
      value={untrack(() => props.value)}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onDblClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.isComposing || e.keyCode === 229) return;
        if (e.key === 'Enter' || e.key === 'NumpadEnter') {
          e.preventDefault();
          finish(true);
          refocus();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          finish(false);
          refocus();
        }
      }}
      onBlur={() => finish(true)}
    />
  );
}
