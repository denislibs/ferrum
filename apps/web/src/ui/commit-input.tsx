// Текстовое поле панели (HEX, прозрачность в процентах): Enter и уход фокуса
// применяют набранное, Esc откатывает. Правила те же, что у поля числа:
// - набранный текст принадлежит пользователю — события ядра его не переписывают;
// - правка адресована тем, кого выделяли, когда начали набирать (`capture`);
//   выделение сменилось, пока в поле набрано: набранное применяется к прежним
//   (а при выделении больше предела id — откатывается, адресата нет);
// - выбор варианта в IME (Enter) — не команда поля.
// Применять ли значение (оно могло не измениться), решает `onCommit`: «то же
// значение» шага отмены не создаёт.

import { createEffect, createSignal, on, type JSX } from 'solid-js';
import { useEditor } from './context';

/** Что нужно полю от снимка выделения (`captureEdit` и его расширения). */
export interface Capture {
  capped: boolean;
  changed(): boolean;
}

export interface CommitInputProps<C extends Capture> {
  /** Название поля для чтения с экрана. */
  name: string;
  /** Показываемый текст (из документа). */
  text: string;
  /** Снимок на начало набора. */
  capture: () => C;
  onCommit: (text: string, capture: C) => void;
  /** Метка перед полем (`#`). */
  prefix?: JSX.Element;
  class?: string;
  maxLength?: number;
}

export function CommitInput<C extends Capture>(props: CommitInputProps<C>) {
  const ed = useEditor();
  const [dirty, setDirty] = createSignal(false);
  let input!: HTMLInputElement;
  let session: C | null = null;

  createEffect(() => {
    const text = props.text;
    if (!dirty()) input.value = text;
  });

  const revert = () => {
    setDirty(false);
    session = null;
    input.value = props.text;
  };
  const commit = () => {
    if (!dirty() || !session) return;
    const s = session;
    const text = input.value;
    revert();
    // Выделение больше предела id сменилось: адресата нет, набранное откатывается.
    if (s.capped && s.changed()) return;
    props.onCommit(text, s);
  };

  createEffect(
    on(
      () => [ed.store.state.selectionIds, ed.store.state.selectionVersion],
      () => {
        if (dirty() && session?.changed()) commit();
      },
      { defer: true },
    ),
  );

  return (
    <label class={`fe-field ${props.class ?? ''}`}>
      {props.prefix}
      <input
        ref={input}
        aria-label={props.name}
        maxLength={props.maxLength ?? 64}
        spellcheck={false}
        onInput={() => {
          session ??= props.capture();
          setDirty(true);
        }}
        onFocus={() => queueMicrotask(() => input.select())}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.isComposing || e.keyCode === 229) return;
          if (e.key === 'Enter' || e.key === 'NumpadEnter') {
            e.preventDefault();
            commit();
            input.select();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            revert();
            input.blur();
          }
        }}
      />
    </label>
  );
}
