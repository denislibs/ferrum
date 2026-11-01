// Меню: выпадающие списки (главное меню, фигуры, пресеты, зум) и контекстное
// меню холста. role="menu" и клавиатура: стрелки (в том числе по серым пунктам —
// они не выполняются), Home/End, Enter/Space, переход по первой букве, Esc
// (фокус возвращается туда, откуда меню открыли), Tab закрывает.
// Подменю (спека подсистемы 2, раздел 4) открываются стрелкой вправо, Enter,
// щелчком и наведением с задержкой, как в Figma; стрелка влево и Esc
// возвращают к пункту-родителю. Доступность и галочка пункта команды — из
// состояния команд ядра (`CommandStates`), подпись сочетания — из действующей
// раскладки. Положение считается в фазе measure: меню сначала появляется
// невидимым, затем встаёт у якоря в пределах окна (подменю у края отражается на
// другую сторону). Вне редактора (экран файлов: ни ядра, ни планировщика) —
// чтение и запись в одном кадре `requestAnimationFrame`, пункты — только с
// `onSelect`.

import { createEffect, createSignal, Index, Match, on, onCleanup, onMount, Show, Switch, useContext, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { EditorContext } from './context';
import { Icon } from './icons';
import { placeMenu, SUBMENU_DELAY_MS, typeAhead } from './menu-logic';
import { shortcutLabel } from './shortcuts';

export type MenuItem =
  | {
      label: string;
      /** Ключ команды редактора: выполнить её, показать её сочетание и состояние. */
      command?: string;
      /** Своя подпись справа (размер пресета). */
      hint?: string;
      /** Явно; без него у команды — из состояния ядра. */
      disabled?: boolean;
      /** Явно; без него у переключателя — из состояния ядра. */
      checked?: boolean;
      onSelect?: () => void;
      /** Подменю. */
      submenu?: MenuItem[];
    }
  | { separator: true }
  | { heading: string };

type Entry = Extract<MenuItem, { label: string }>;

export interface MenuSearch {
  label: string;
  placeholder: string;
  /** Пункты по запросу (непустому). */
  results: (query: string) => MenuItem[];
}

export interface MenuProps {
  items: MenuItem[];
  /** Элемент, под которым открыть, или точка (контекстное меню). */
  anchor: HTMLElement | { x: number; y: number };
  label: string;
  /** Открыть над якорем (тулбар внизу экрана). */
  above?: boolean;
  onClose: () => void;
  /** Поле поиска вверху (главное меню): результаты вместо пунктов. */
  search?: MenuSearch;
}

let trees = 0;

export function Menu(props: MenuProps): JSX.Element {
  const returnFocus = document.activeElement as HTMLElement | null;
  const tree = `menu-${++trees}`;
  const closeAll = (restore: boolean) => {
    props.onClose();
    if (restore) returnFocus?.focus?.();
  };
  onMount(() => {
    const outside = (e: PointerEvent) => {
      const t = e.target instanceof Element ? e.target : null;
      // Якорь-кнопка сама закрывает меню повторным щелчком.
      const anchor = props.anchor instanceof HTMLElement && t && props.anchor.contains(t);
      if (!(t?.closest(`[data-menu-tree="${tree}"]`) || anchor)) closeAll(false);
    };
    // Нажатие, которым меню открыли, ещё всплывает: слушаем со следующей задачи.
    const t = setTimeout(() => document.addEventListener('pointerdown', outside, true));
    window.addEventListener('blur', props.onClose);
    onCleanup(() => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', outside, true);
      window.removeEventListener('blur', props.onClose);
    });
  });
  return (
    <Level
      items={props.items}
      anchor={props.anchor}
      label={props.label}
      above={props.above}
      search={props.search}
      tree={tree}
      depth={0}
      focusFirst
      closeAll={closeAll}
      closeSelf={() => closeAll(true)}
    />
  );
}

interface LevelProps {
  items: MenuItem[];
  anchor: HTMLElement | { x: number; y: number };
  label: string;
  above?: boolean;
  search?: MenuSearch;
  tree: string;
  depth: number;
  /** Фокус на первый пункт при открытии (клавиатура); наведение фокус не трогает. */
  focusFirst: boolean;
  closeAll: (restore: boolean) => void;
  /** Закрыть этот уровень (у корня — всё меню). */
  closeSelf: () => void;
  /** Указатель вошёл в подменю: родитель не закрывает его по таймеру. */
  onHover?: () => void;
}

function Level(props: LevelProps): JSX.Element {
  const ed = useContext(EditorContext);
  let el!: HTMLDivElement;
  let input: HTMLInputElement | undefined;
  const [query, setQuery] = createSignal('');
  /** Открытое подменю: номер пункта, его элемент и нужен ли фокус в подменю. */
  const [sub, setSub] = createSignal<{ index: number; el: HTMLElement; focus: boolean } | null>(null);
  let timer = 0;
  onCleanup(() => clearTimeout(timer));

  const shown = (): MenuItem[] => (props.search && query().trim() ? props.search.results(query()) : props.items);
  const items = () => Array.from(el.querySelectorAll<HTMLElement>(':scope > [data-index]'));
  const state = (it: Entry) => (it.command && ed ? ed.store.state.commands[it.command] : undefined);
  const disabled = (it: Entry) => it.disabled ?? (state(it) ? !state(it)!.enabled : false);
  const checked = (it: Entry) => it.checked ?? state(it)?.checked ?? undefined;
  const entry = (i: number) => {
    const it = shown()[i];
    return it && 'label' in it ? it : undefined;
  };

  const openSub = (index: number, target: HTMLElement, focus: boolean) => {
    clearTimeout(timer);
    const cur = sub();
    if (cur && cur.index === index) {
      if (focus) setSub({ ...cur, focus: true });
      return;
    }
    setSub({ index, el: target, focus });
  };
  const closeSub = (refocus: boolean) => {
    clearTimeout(timer);
    const cur = sub();
    setSub(null);
    if (refocus) cur?.el.focus();
  };

  const select = (i: number, target: HTMLElement) => {
    const it = entry(i);
    if (!it) return;
    if (it.submenu) {
      openSub(i, target, true);
      return;
    }
    if (disabled(it)) return;
    props.closeAll(true);
    if (it.onSelect) it.onSelect();
    else if (it.command) ed?.run(it.command);
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.target === input) {
      // Поле поиска: стрелка вниз — к пунктам, Enter — первый доступный.
      if (e.key === 'ArrowDown') {
        items()[0]?.focus();
      } else if (e.key === 'Enter') {
        const list = items();
        const first = list.find((x) => x.getAttribute('aria-disabled') !== 'true' && !x.hasAttribute('aria-haspopup'));
        if (first) select(Number(first.dataset.index), first);
      } else if (e.key === 'Escape') {
        props.closeSelf();
      } else if (e.key === 'Tab') {
        props.closeAll(false);
        return;
      } else return;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    const current = at >= 0 ? list[at] : undefined;
    switch (e.key) {
      case 'ArrowDown':
        go(at + 1);
        break;
      case 'ArrowUp':
        // С первого пункта главного меню — обратно в поле поиска.
        if (at === 0 && input) input.focus();
        else go(at - 1);
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(list.length - 1);
        break;
      case 'ArrowRight':
        if (current?.hasAttribute('aria-haspopup')) openSub(Number(current.dataset.index), current, true);
        break;
      case 'ArrowLeft':
        if (props.depth > 0) props.closeSelf();
        break;
      case 'Enter':
      case ' ':
        if (current) select(Number(current.dataset.index), current);
        break;
      case 'Escape':
        props.closeSelf();
        break;
      case 'Tab':
        props.closeAll(false);
        return;
      default: {
        // Переход по первой букве (без модификаторов).
        if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
        const labels = list.map((x) => x.dataset.label ?? '');
        const i = typeAhead(labels, at, e.key);
        if (i >= 0) list[i].focus();
      }
    }
    e.preventDefault();
    e.stopPropagation();
  };

  // Подменю, открытое наведением, получает фокус, когда в него входят с клавиатуры.
  let placed = false;
  createEffect(
    on(
      () => props.focusFirst,
      (f) => {
        if (f && placed) items()[0]?.focus();
      },
      { defer: true },
    ),
  );

  onMount(() => {
    const measure = ed ? ed.scheduler.measure : (fn: () => void) => requestAnimationFrame(fn);
    const mutate = ed ? ed.scheduler.mutate : (fn: () => void) => fn();
    measure(() => {
      const box = el.getBoundingClientRect();
      const a = props.anchor;
      const r = a instanceof HTMLElement ? a.getBoundingClientRect() : { left: a.x, right: a.x, top: a.y, bottom: a.y };
      const { x, y } = placeMenu(r, box, { width: innerWidth, height: innerHeight }, { above: props.above, submenu: props.depth > 0 });
      mutate(() => {
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        el.style.visibility = 'visible';
        placed = true;
        if (input) input.focus();
        else if (props.focusFirst) items()[0]?.focus();
      });
    });
  });

  const key = (command?: string) => {
    if (!command || !ed) return '';
    const c = ed.store.state.keymap.commands.find((x) => x.key === command);
    return c?.bindings[0] ? shortcutLabel(c.bindings[0], ed.platform, ed.layout()) : '';
  };

  return (
    <Portal>
      <div
        ref={el}
        class="fe-menu"
        role="menu"
        aria-label={props.label}
        data-menu-tree={props.tree}
        // Невидимым — в углу окна: размер не зависит от места в потоке документа.
        style={{ visibility: 'hidden', left: '0px', top: '0px' }}
        onKeyDown={onKey}
        onPointerEnter={() => props.onHover?.()}
      >
        <Show when={props.search}>
          {(s) => (
            <div class="fe-menu__search">
              <Icon name="search" />
              <input
                ref={input}
                type="search"
                aria-label={s().label}
                placeholder={s().placeholder}
                value={query()}
                onInput={(e) => {
                  closeSub(false);
                  setQuery(e.currentTarget.value);
                }}
              />
            </div>
          )}
        </Show>
        <Show when={props.search && query().trim() && shown().length === 0}>
          <div class="fe-menu__heading fe-menu__empty">Ничего не найдено</div>
        </Show>
        {/* Index, не For: при перестройке пунктов (зум изменился) узлы остаются, фокус не теряется. */}
        <Index each={shown()}>
          {(item, i) => (
            <Switch>
              <Match when={'separator' in item() && item()}>
                <div class="fe-menu__sep" role="separator" />
              </Match>
              <Match when={'heading' in item() && (item() as { heading: string })}>
                {(h) => <div class="fe-menu__heading">{h().heading}</div>}
              </Match>
              <Match when={'label' in item() && (item() as Entry)}>
                {(it) => (
                  <div
                    class="fe-menu__item"
                    role={checked(it()) === undefined ? 'menuitem' : 'menuitemcheckbox'}
                    aria-checked={checked(it()) === undefined ? undefined : checked(it())}
                    aria-disabled={!it().submenu && disabled(it()) ? 'true' : undefined}
                    aria-haspopup={it().submenu ? 'menu' : undefined}
                    aria-expanded={it().submenu ? sub()?.index === i : undefined}
                    data-index={i}
                    data-label={it().label}
                    data-command={it().command}
                    tabindex="-1"
                    onClick={(e) => select(i, e.currentTarget)}
                    onPointerEnter={(e) => {
                      // Наведение: подменю открывается с задержкой, чужое — закрывается.
                      clearTimeout(timer);
                      const target = e.currentTarget;
                      if (it().submenu) timer = window.setTimeout(() => openSub(i, target, false), SUBMENU_DELAY_MS);
                      else if (sub()) timer = window.setTimeout(() => closeSub(false), SUBMENU_DELAY_MS);
                    }}
                  >
                    <span class="fe-menu__check">
                      <Show when={checked(it())}>
                        <Icon name="check" />
                      </Show>
                    </span>
                    <span class="fe-menu__label">{it().label}</span>
                    <span class="fe-menu__key">
                      <Show when={it().submenu} fallback={it().hint ?? key(it().command)}>
                        <Icon name="expand" />
                      </Show>
                    </span>
                  </div>
                )}
              </Match>
            </Switch>
          )}
        </Index>
      </div>
      <Show when={sub()}>
        {(s) => (
          <Level
            items={entry(s().index)?.submenu ?? []}
            anchor={s().el}
            label={entry(s().index)?.label ?? ''}
            tree={props.tree}
            depth={props.depth + 1}
            focusFirst={s().focus}
            closeAll={props.closeAll}
            closeSelf={() => closeSub(true)}
            onHover={() => clearTimeout(timer)}
          />
        )}
      </Show>
    </Portal>
  );
}
