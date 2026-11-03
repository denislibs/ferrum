// «Быстрые действия» (спека подсистемы 2, раздел 7; Ctrl/Cmd+K и Ctrl/Cmd+/, как
// в Figma): поле сверху по центру холста и один список с заголовками — недавние,
// команды, слои, страницы. Фокус остаётся в поле (комбобокс и `aria-activedescendant`):
// стрелки ходят по строкам, в том числе по серым, Enter выполняет, Esc и щелчок мимо
// закрывают, фокус возвращается туда, откуда открыли. Команда выполняется, как из
// главного меню (`ed.run`, включая `ui.*`). Слои ищет worker: каждый символ — новый
// запрос `FindLayers`, ответы на прежние отбрасываются по `query`. Положение задаёт
// CSS; прокрутка к строке читает раскладку только в фазе measure.

import { createEffect, createMemo, createSignal, Index, Match, on, onCleanup, onMount, Show, Switch, type JSX } from 'solid-js';
import type { FoundLayerDto, KeymapEntry } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { opensWindow } from './keys';
import { Icon } from './icons';
import { GROUPS } from './keymap-ui';
import { KIND_ICON } from './layers';
import { entriesByKey } from './main-menu';
import { clipQuery, LAYER_LIMIT, paletteRows, type PaletteRow } from './palette';
import { eventShortcut, shortcutLabel } from './shortcuts';

type Action = Exclude<PaletteRow, { kind: 'heading' }>;

const GROUP_TITLE = new Map(GROUPS.map((g) => [g.id, g.title]));

export interface QuickActionsProps {
  /** Команды, которых нет на этой платформе. */
  hidden: ReadonlySet<string>;
  /** Недавние команды, новые первыми. */
  recent: readonly string[];
  onClose: () => void;
}

export function QuickActions(props: QuickActionsProps): JSX.Element {
  const ed = useEditor();
  const returnFocus = document.activeElement as HTMLElement | null;
  const [query, setQuery] = createSignal('');
  /** Последний запрос к worker'у (пусто — слои не ищем) и ответ на него, если пришёл. */
  const [asked, setAsked] = createSignal('');
  const [answer, setAnswer] = createSignal<{ query: string; items: readonly FoundLayerDto[] }>({ query: '', items: [] });
  /** Ответ на текущий запрос ещё не пришёл: слоёв нет, «Ничего не найдено» рано. */
  const pending = () => asked() !== '' && answer().query !== asked();
  /** Номер строки с действием среди строк без заголовков. */
  const [active, setActive] = createSignal(0);
  let root!: HTMLDivElement;
  let list!: HTMLDivElement;
  let input!: HTMLInputElement;
  let lastQuery = '';

  const entries = createMemo(() => entriesByKey(ed.store.state.keymap.commands));
  const rows = createMemo(() =>
    paletteRows({
      query: query(),
      commands: ed.store.state.keymap.commands,
      hidden: props.hidden,
      recent: props.recent,
      // Слои прежнего запроса не показываем и не даём выполнить.
      layers: pending() ? [] : answer().items,
      pages: ed.store.state.pages.list,
    }),
  );
  const actions = createMemo(() => rows().filter((r): r is Action => r.kind !== 'heading'));
  /** Номер строки с действием для строки списка (заголовки не считаются). */
  const actionIndex = createMemo(() => {
    let n = 0;
    return rows().map((r) => (r.kind === 'heading' ? -1 : n++));
  });
  const disabled = (r: Action) => r.kind === 'command' && ed.store.state.commands[r.key]?.enabled === false;

  const close = (restore: boolean) => {
    props.onClose();
    if (restore) returnFocus?.focus?.();
  };
  const execute = (r: Action | undefined) => {
    if (!r || disabled(r)) return;
    close(true);
    if (r.kind === 'command') {
      ed.noteRecent?.(r.key);
      ed.run(r.key);
    } else if (r.kind === 'layer') ed.send({ type: 'revealLayer', id: r.id });
    else ed.send({ type: 'setPage', id: r.id });
  };

  // Выбранная строка помнится по смыслу (команда, слой, страница): поздний ответ
  // worker'а её не сбрасывает. Первая доступная — при новом тексте запроса и когда
  // строки больше нет.
  const identity = (r: Action | undefined) => (r ? (r.kind === 'command' ? `c:${r.key}` : `${r.kind}:${r.id}`) : '');
  createEffect(
    on(actions, (a, prev) => {
      const q = query();
      const was = q === lastQuery ? identity(prev?.[active()]) : '';
      lastQuery = q;
      const kept = was ? a.findIndex((r) => identity(r) === was) : -1;
      setActive(kept >= 0 ? kept : Math.max(0, a.findIndex((r) => !disabled(r))));
    }),
  );
  // Выбранная строка — в видимой части списка: чтение в measure, запись в mutate.
  createEffect(
    on(active, (i) => {
      ed.scheduler.measure(() => {
        const el = list?.querySelector<HTMLElement>(`[data-action="${i}"]`);
        if (!el) return;
        const top = el.offsetTop;
        const bottom = top + el.offsetHeight;
        const { scrollTop, clientHeight } = list;
        if (top < scrollTop) ed.scheduler.mutate(() => (list.scrollTop = top));
        else if (bottom > scrollTop + clientHeight) ed.scheduler.mutate(() => (list.scrollTop = bottom - clientHeight));
      });
    }),
  );

  const search = (q: string) => {
    setQuery(q);
    const sent = clipQuery(q.trim());
    setAsked(sent);
    if (sent) ed.send({ type: 'findLayers', query: sent, limit: LAYER_LIMIT });
  };

  const isOwn = (e: KeyboardEvent) => {
    const s = eventShortcut(e);
    return !!s && opensWindow(e) && !!entries().get('ui.quick_actions')?.bindings.includes(s);
  };

  const onKey = (e: KeyboardEvent) => {
    // Составление IME: Enter, Esc и стрелки — его.
    if (e.isComposing || e.keyCode === 229) return;
    const n = actions().length;
    switch (e.key) {
      case 'ArrowDown':
        if (n) setActive((active() + 1) % n);
        break;
      case 'ArrowUp':
        if (n) setActive((active() - 1 + n) % n);
        break;
      case 'Enter':
        // Удержание Enter выполняет один раз.
        if (!e.repeat) execute(actions()[active()]);
        break;
      case 'Escape':
        close(true);
        break;
      case 'Tab':
        close(false);
        return;
      default:
        // Сочетание палитры в её же поле закрывает её, как в Figma.
        if (!isOwn(e)) return;
        if (!e.repeat) close(true);
    }
    e.preventDefault();
    e.stopPropagation();
  };

  onMount(() => {
    input.focus();
    // Ответы — через очередь планировщика; ответ на прежний запрос не нужен.
    const off = ed.core.on((e) => {
      if (e.type !== 'layersFound') return;
      ed.scheduler.enqueue(() => {
        if (e.query === asked()) setAnswer({ query: e.query, items: e.items });
      });
    });
    const outside = (e: PointerEvent) => {
      if (!(e.target instanceof Node && root.contains(e.target))) close(false);
    };
    // Фокус ушёл из поля (его не должно: щелчки внутри его не забирают): Esc и
    // сочетание палитры всё равно закрывают, остальные клавиши холст не получает.
    const stray = (e: KeyboardEvent) => {
      if (e.target === input || e.isComposing || e.keyCode === 229) return;
      e.stopPropagation();
      if (e.key !== 'Escape' && !isOwn(e)) return;
      e.preventDefault();
      if (!e.repeat) close(true);
    };
    document.addEventListener('pointerdown', outside, true);
    document.addEventListener('keydown', stray, true);
    onCleanup(() => {
      off();
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('keydown', stray, true);
    });
  });

  const key = (k: string) => {
    const e: KeymapEntry | undefined = entries().get(k);
    return e?.bindings[0] ? shortcutLabel(e.bindings[0], ed.platform, ed.layout()) : '';
  };
  const optionId = (i: number) => `fe-palette-option-${i}`;

  const Row = (p: { row: () => Action; at: () => number }) => {
    const entry = () => {
      const r = p.row();
      return r.kind === 'command' ? entries().get(r.key) : undefined;
    };
    return (
      <div
        id={optionId(p.at())}
        class="fe-palette__row"
        classList={{ 'fe-palette__row--active': active() === p.at() }}
        role="option"
        aria-selected={active() === p.at()}
        aria-disabled={disabled(p.row()) ? 'true' : undefined}
        data-action={p.at()}
        data-command={entry()?.key}
        onPointerMove={() => {
          if (active() !== p.at()) setActive(p.at());
        }}
        onClick={() => execute(p.row())}
      >
        <span class="fe-palette__icon">
          <Switch>
            <Match when={p.row().kind === 'layer' && (p.row() as Extract<Action, { kind: 'layer' }>)}>
              {(l) => <Icon name={KIND_ICON[l().type]} />}
            </Match>
            <Match when={p.row().kind === 'page'}>
              <Icon name="file" />
            </Match>
            <Match when={entry() && ed.store.state.commands[entry()!.key]?.checked}>
              <Icon name="check" />
            </Match>
          </Switch>
        </span>
        <span class="fe-palette__name">{entry()?.name ?? (p.row() as Exclude<Action, { kind: 'command' }>).name}</span>
        <Show when={entry()}>
          {(c) => (
            <>
              <span class="fe-palette__group">{GROUP_TITLE.get(c().group) ?? c().group}</span>
              <span class="fe-palette__key">{key(c().key)}</span>
            </>
          )}
        </Show>
      </div>
    );
  };

  return (
    // Щелчок внутри окна не забирает фокус из поля (кроме самого поля).
    <div
      ref={root}
      class="fe-palette"
      role="dialog"
      aria-label="Быстрые действия"
      onPointerDown={(e) => {
        if (e.target !== input) e.preventDefault();
      }}
    >
      <div class="fe-palette__field">
        <Icon name="search" />
        <input
          ref={input}
          type="text"
          role="combobox"
          aria-label="Быстрые действия"
          aria-expanded="true"
          aria-controls="fe-palette-list"
          aria-autocomplete="list"
          aria-activedescendant={actions().length ? optionId(active()) : undefined}
          placeholder="Команда, слой или страница"
          autocomplete="off"
          spellcheck={false}
          value={query()}
          onInput={(e) => search(e.currentTarget.value)}
          onKeyDown={onKey}
        />
      </div>
      <div ref={list} id="fe-palette-list" class="fe-palette__list" role="listbox" aria-label="Результаты">
        <Show when={rows().length === 0 && !pending()}>
          <div class="fe-palette__empty">Ничего не найдено</div>
        </Show>
        {/* Index, не For: узлы строк остаются, ввод не пересоздаёт список. */}
        <Index each={rows()}>
          {(row, i) => (
            <Switch>
              <Match when={row().kind === 'heading' && (row() as Extract<PaletteRow, { kind: 'heading' }>)}>
                {(h) => (
                  <div class="fe-palette__heading" role="presentation">
                    {h().title}
                  </div>
                )}
              </Match>
              <Match when={row().kind !== 'heading'}>
                <Row row={() => row() as Action} at={() => actionIndex()[i]} />
              </Match>
            </Switch>
          )}
        </Index>
      </div>
    </div>
  );
}
