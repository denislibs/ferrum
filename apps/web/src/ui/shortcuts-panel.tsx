// Панель «Сочетания клавиш» (спека подсистемы 2, раздел 8; Ctrl+Shift+? и «Справка →
// Сочетания клавиш», как в Figma): поднимается снизу окна, вкладки по группам, у
// каждой команды — клавиши из действующей раскладки, использованные недавно отмечены,
// ссылка «Изменить в настройках». Данные — те же `KeymapEntry`, что у вкладки настроек.
// Окно не модальное: холст доступен, клавиши в самой панели — её (`keys.ts`); Esc
// закрывает панель, если фокус в ней, сочетание панели закрывает её и с холста.
// Раскладку панель не читает: положение и размер задаёт CSS.

import { createMemo, createSignal, For, onMount, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
import { Icon } from './icons';
import { groupCommands } from './keymap-ui';
import { eventShortcut, shortcutLabel } from './shortcuts';

export interface ShortcutsPanelProps {
  /** Команды, использованные недавно: они отмечены. */
  used: ReadonlySet<string>;
  /** Открыть настройки на вкладке сочетаний (панель уже закрылась). */
  onSettings: () => void;
  onClose: () => void;
}

export function ShortcutsPanel(props: ShortcutsPanelProps): JSX.Element {
  const ed = useEditor();
  const returnFocus = document.activeElement as HTMLElement | null;
  let root!: HTMLDivElement;
  const groups = createMemo(() => groupCommands(ed.store.state.keymap.commands));
  const [tab, setTab] = createSignal(groups()[0]?.id ?? '');
  const current = () => groups().find((g) => g.id === tab()) ?? groups()[0];
  const label = (s: string) => shortcutLabel(s, ed.platform, ed.layout());

  const close = () => {
    // Фокус ушёл на холст (панель не модальная) — его не трогаем.
    const inside = root.contains(document.activeElement);
    props.onClose();
    if (inside) returnFocus?.focus?.();
  };

  const onKey = (e: KeyboardEvent) => {
    const s = eventShortcut(e);
    const own = s && ed.store.state.keymap.commands.find((c) => c.key === 'ui.shortcuts')?.bindings.includes(s);
    if (e.key === 'Escape' || own) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };

  const focusTab = (id: string) => root.querySelector<HTMLElement>(`[data-group="${id}"]`)?.focus();
  const onTabKey = (e: KeyboardEvent) => {
    const list = groups();
    const i = list.findIndex((g) => g.id === tab());
    const next =
      e.key === 'ArrowRight' ? (i + 1) % list.length
      : e.key === 'ArrowLeft' ? (i - 1 + list.length) % list.length
      : e.key === 'Home' ? 0
      : e.key === 'End' ? list.length - 1
      : -1;
    if (next < 0) return;
    e.preventDefault();
    setTab(list[next].id);
    focusTab(list[next].id);
  };

  onMount(() => focusTab(tab()));

  return (
    <div ref={root} class="fe-keys" role="dialog" aria-label="Сочетания клавиш" onKeyDown={onKey}>
      <div class="fe-keys__head">
        <h2 class="t-strong">Сочетания клавиш</h2>
        <div class="fe-tabs" role="tablist" aria-label="Группы сочетаний">
          <For each={groups()}>
            {(g) => (
              <button
                class="fe-tab"
                role="tab"
                data-group={g.id}
                aria-selected={tab() === g.id}
                aria-controls="fe-keys-list"
                tabindex={tab() === g.id ? 0 : -1}
                onClick={() => setTab(g.id)}
                onKeyDown={onTabKey}
              >
                {g.title}
              </button>
            )}
          </For>
        </div>
        <button
          class="fe-keys__settings"
          onClick={() => {
            close();
            props.onSettings();
          }}
        >
          Изменить в настройках
        </button>
        <button class="fe-tool fe-tool--small" aria-label="Закрыть сочетания клавиш" data-tip="Закрыть" data-tip-key="Esc" onClick={close}>
          <Icon name="close" />
        </button>
      </div>
      <div id="fe-keys-list" class="fe-keys__list" role="tabpanel" aria-label={current()?.title}>
        <For each={current()?.commands ?? []}>
          {(c) => (
            <div class="fe-keys__row" classList={{ 'fe-keys__row--used': props.used.has(c.key) }} data-command={c.key}>
              <span class="fe-keys__name">
                {c.name}
                <Show when={props.used.has(c.key)}>
                  <span class="fe-keys__mark" role="img" aria-label="использовалась недавно" title="Использовалась недавно" />
                </Show>
              </span>
              <span class="fe-keys__caps">
                <For each={c.bindings} fallback={<span class="fe-keys__none">—</span>}>
                  {(b) => <kbd>{label(b)}</kbd>}
                </For>
              </span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
