// Всплывающие подсказки: у каждой кнопки-иконки — «Название  Клавиша», клавиша
// из действующей раскладки (переназначили — подсказка сразу другая). Один слой на
// весь интерфейс, делегирование событий: элемент с `data-tip` (и `data-command` —
// ключ команды, чьё сочетание показать, или `data-tip-key` — клавиша не из
// раскладки, например Esc). Положение считается в фазе measure.

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { useEditor } from './context';
import { shortcutLabel, tipText } from './shortcuts';

/** Задержка перед показом, как у системных подсказок. */
export const TOOLTIP_DELAY_MS = 500;

interface Tip {
  text: string;
  command: string;
  /** Клавиша не из раскладки (`data-tip-key`). */
  key: string;
  x: number;
  y: number;
  above: boolean;
}

export function TooltipLayer() {
  const ed = useEditor();
  const [tip, setTip] = createSignal<Tip | null>(null);
  let timer = 0;
  let target: HTMLElement | null = null;

  // Читается из состояния в разметке: открытая подсказка следует за раскладкой.
  const keyOf = (command: string | undefined): string => {
    if (!command) return '';
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    const first = c?.bindings[0];
    return first ? shortcutLabel(first, ed.platform, ed.layout()) : '';
  };

  const hide = () => {
    clearTimeout(timer);
    target = null;
    setTip(null);
  };

  const show = (el: HTMLElement, delay: number) => {
    clearTimeout(timer);
    target = el;
    timer = window.setTimeout(() => {
      ed.scheduler.measure(() => {
        if (target !== el || !el.isConnected) return;
        const r = el.getBoundingClientRect();
        const above = el.dataset.tipSide === 'top' || r.bottom + 40 > innerHeight;
        const x = Math.min(Math.max(r.left + r.width / 2, 80), innerWidth - 80);
        const text = el.dataset.tip ?? '';
        const command = el.dataset.command ?? '';
        const key = el.dataset.tipKey ?? '';
        ed.scheduler.mutate(() => setTip({ text, command, key, x, y: above ? r.top - 6 : r.bottom + 6, above }));
      });
    }, delay);
  };

  const tipped = (t: EventTarget | null) => (t instanceof Element ? (t.closest('[data-tip]') as HTMLElement | null) : null);

  onMount(() => {
    const over = (e: PointerEvent) => {
      const el = tipped(e.target);
      if (el && el !== target) show(el, TOOLTIP_DELAY_MS);
      if (!el && target) hide();
    };
    const focus = (e: FocusEvent) => {
      const el = tipped(e.target);
      // Фокус с клавиатуры — подсказка сразу; фокус мышью — без неё.
      if (el && el.matches(':focus-visible')) show(el, 0);
    };
    document.addEventListener('pointerover', over);
    document.addEventListener('pointerdown', hide, true);
    document.addEventListener('focusin', focus);
    document.addEventListener('focusout', hide);
    document.addEventListener('keydown', hide, true);
    onCleanup(() => {
      document.removeEventListener('pointerover', over);
      document.removeEventListener('pointerdown', hide, true);
      document.removeEventListener('focusin', focus);
      document.removeEventListener('focusout', hide);
      document.removeEventListener('keydown', hide, true);
      clearTimeout(timer);
    });
  });

  return (
    <Show when={tip()}>
      {(t) => (
        <div
          class="fe-tooltip"
          role="tooltip"
          style={{
            left: `${t().x}px`,
            top: `${t().y}px`,
            // Отдельное свойство `translate`, не `transform`: анимация появления
            // (fe-pop) двигает `transform`, и на первом кадре затирала бы сдвиг —
            // подсказка «прилетала» сбоку, а не появлялась на месте.
            translate: `-50% ${t().above ? '-100%' : '0'}`,
          }}
        >
          {/* «Название  Клавиша»: два пробела — часть текста (white-space: pre). */}
          <span>{t().text}</span>
          <Show when={keyOf(t().command) || t().key}>
            {(key) => <span class="fe-tooltip__key">{tipText('', key())}</span>}
          </Show>
        </div>
      )}
    </Show>
  );
}
