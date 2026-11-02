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
  /** Ответ worker'а на последний отправленный запрос. */
  const [layers, setLayers] = createSignal<readonly FoundLayerDto[]>([]);
  /** Номер строки с действием среди строк без заголовков. */
  const [active, setActive] = createSignal(0);
  let root!: HTMLDivElement;
  let list!: HTMLDivElement;
  let input!: HTMLInputElement;
  let sent = '';

  const entries = createMemo(() => entriesByKey(ed.store.state.keymap.commands));
  const rows = createMemo(() =>
    paletteRows({
      query: query(),
      commands: ed.store.state.keymap.commands,
      hidden: props.hidden,
      recent: props.recent,
      layers: layers(),
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

  // Новый список — выбрана первая доступная строка.
  createEffect(
    on(actions, (a) => {
      const i = a.findIndex((r) => !disabled(r));
      setActive(Math.max(0, i));
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
    sent = clipQuery(q.trim());
    if (sent) ed.send({ type: 'findLayers', query: sent, limit: LAYER_LIMIT });
    else setLayers([]);
  };

  const onKey = (e: KeyboardEvent) => {
    const n = actions().length;
    switch (e.key) {
      case 'ArrowDown':
        if (n) setActive((active() + 1) % n);
        break;
      case 'ArrowUp':
        if (n) setActive((active() - 1 + n) % n);
        break;
      case 'Enter':
        execute(actions()[active()]);
        break;
      case 'Escape':
        close(true);
        break;
      case 'Tab':
        close(false);
        return;
      default: {
        // Сочетание палитры в её же поле закрывает её, как в Figma.
        const s = eventShortcut(e);
        if (!s || !entries().get('ui.quick_actions')?.bindings.includes(s)) return;
        close(true);
      }
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
        if (e.query === sent) setLayers(e.items);
      });
    });
    const outside = (e: PointerEvent) => {
      if (!(e.target instanceof Node && root.contains(e.target))) close(false);
    };
    document.addEventListener('pointerdown', outside, true);
    onCleanup(() => {
      off();
      document.removeEventListener('pointerdown', outside, true);
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
    <div ref={root} class="fe-palette" role="dialog" aria-label="Быстрые действия">
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
      {/* Щелчок по списку не забирает фокус из поля. */}
      <div ref={list} id="fe-palette-list" class="fe-palette__list" role="listbox" aria-label="Результаты" onPointerDown={(e) => e.preventDefault()}>
        <Show when={rows().length === 0}>
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
