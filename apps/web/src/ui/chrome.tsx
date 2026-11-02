// Шапки панелей (PanelHeader), главное меню, зум, «Поделиться», вкладки, края
// панелей для изменения ширины и контекстное меню холста.

import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem, type MenuSearch } from './menu';
import { RenameInput } from './rename-input';
import { roving } from './roving';
import { clampPanel, PANEL_MAX, PANEL_MIN } from './settings';

export { clampPanel, PANEL_MAX, PANEL_MIN };

/** Шапка левой панели: кнопка с логотипом открывает главное меню (как в Figma
 * UI3; оно же — по `ui.main_menu`, Alt+/), имя файла, «Назад к файлам».
 * Открыто ли меню, решает редактор (`open`): его открывает и сочетание. */
export function FileHeader(props: {
  menu: () => MenuItem[];
  search: MenuSearch;
  open: boolean;
  onOpen: (open: boolean) => void;
  onBack?: () => void;
}) {
  const ed = useEditor();
  const [editing, setEditing] = createSignal(false);
  let file!: HTMLDivElement;
  let logo!: HTMLButtonElement;
  return (
    <div class="fe-panel-head">
      <button
        ref={logo}
        class="fe-logo"
        aria-haspopup="menu"
        aria-expanded={props.open}
        aria-label="Главное меню"
        data-tip="Главное меню"
        data-command="ui.main_menu"
        onClick={() => props.onOpen(!props.open)}
      >
        <span class="fe-panel-head__mark" aria-hidden="true" />
        <Caret />
      </button>
      <div ref={file} class="fe-panel-head__file">
        <Show
          when={editing()}
          fallback={
            // Имя — кнопка: двойной щелчок, Enter или F2 открывают переименование.
            <button
              class="fe-panel-head__name"
              title={ed.store.state.documentName}
              aria-keyshortcuts="Enter F2"
              onDblClick={() => setEditing(true)}
              onKeyDown={(e) => {
                if (e.key !== 'Enter' && e.key !== 'F2') return;
                e.preventDefault();
                setEditing(true);
              }}
            >
              {ed.store.state.documentName}
            </button>
          }
        >
          <RenameInput
            label="Имя файла"
            value={ed.store.state.documentName}
            onCommit={(name) => {
              if (name.trim() && name !== ed.store.state.documentName) ed.send({ type: 'setDocumentName', name });
            }}
            onClose={() => setEditing(false)}
            returnFocus={() => file.querySelector<HTMLElement>('.fe-panel-head__name')}
          />
        </Show>
        <span class="fe-panel-head__project">Черновики</span>
      </div>
      <Show when={props.onBack}>
        {(back) => (
          <button class="fe-tool fe-tool--small fe-tool--muted" aria-label="Назад к файлам" data-tip="Назад к файлам" onClick={() => back()()}>
            <Icon name="back" />
          </button>
        )}
      </Show>
      <Show when={props.open}>
        <Menu label="Главное меню" items={props.menu()} anchor={logo} search={props.search} onClose={() => props.onOpen(false)} />
      </Show>
    </div>
  );
}

const ZOOMS = [0.5, 1, 2];

export function DesignHeader() {
  const ed = useEditor();
  let tabs!: HTMLDivElement;
  onMount(() => onCleanup(roving(tabs, '[role="tab"]')));
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const zoomItems = (): MenuItem[] => [
    { label: 'Приблизить', command: 'view.zoom_in' },
    { label: 'Отдалить', command: 'view.zoom_out' },
    { separator: true },
    { label: 'Показать всё', command: 'view.fit_all' },
    { label: 'Показать выделение', command: 'view.fit_selection' },
    { separator: true },
    ...ZOOMS.map((z) => ({
      label: `Масштаб ${z * 100}%`,
      command: z === 1 ? 'view.zoom_100' : undefined,
      checked: Math.abs(ed.store.state.live.zoom - z) < 1e-6,
      onSelect: () => ed.send({ type: 'setZoom', zoom: z }),
    })),
  ];
  return (
    <>
      <div class="fe-panel-head">
        <span class="fe-panel-head__spacer" />
        <button
          class="fe-zoom"
          aria-haspopup="menu"
          aria-expanded={!!menu()}
          aria-label="Масштаб"
          data-tip="Масштаб"
          onClick={(e) => setMenu(e.currentTarget)}
        >
          <span class="t-num">{Math.round(ed.store.state.live.zoom * 100)}%</span>
          <Caret />
        </button>
        <button
          class="fe-btn fe-btn--primary"
          aria-disabled="true"
          data-tip="Совместная работа появится в подсистеме 4"
        >
          Поделиться
        </button>
        <Show when={menu()}>
          {(a) => <Menu label="Масштаб" items={zoomItems()} anchor={a()} onClose={() => setMenu(null)} />}
        </Show>
      </div>
      <div ref={tabs} class="fe-tabs" role="tablist" aria-label="Режим панели">
        <button class="fe-tab" role="tab" aria-selected="true" id="tab-design">
          Дизайн
        </button>
        <button class="fe-tab" role="tab" aria-selected="false" aria-disabled="true" data-tip="Прототипы появятся в подсистеме 6">
          Прототип
        </button>
      </div>
    </>
  );
}

/** Край панели: перетаскивание меняет ширину, стрелки с клавиатуры — на 8 px. */
export function ResizeEdge(props: { side: 'left' | 'right'; width: number; onWidth: (w: number) => void }) {
  const ed = useEditor();
  let start = 0;
  let base = 0;
  let edge: HTMLDivElement | null = null;
  const stop = () => {
    edge?.removeEventListener('pointermove', move);
    edge = null;
  };
  const move = ed.core.costed((e: PointerEvent) => {
    // Кнопку отпустили, а pointerup не пришёл (за окном): перетаскивание кончилось.
    if (e.buttons === 0) {
      stop();
      return;
    }
    const dx = e.clientX - start;
    const w = clampPanel(props.side === 'left' ? base + dx : base - dx);
    ed.scheduler.mutate(() => props.onWidth(w));
  });
  return (
    <div
      class={`fe-resize fe-resize--${props.side}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.side === 'left' ? 'Ширина левой панели' : 'Ширина правой панели'}
      aria-valuemin={PANEL_MIN}
      aria-valuemax={PANEL_MAX}
      aria-valuenow={props.width}
      tabindex="0"
      onPointerDown={(e) => {
        if (e.button !== 0 || !e.isPrimary) return;
        start = e.clientX;
        base = props.width;
        e.currentTarget.setPointerCapture(e.pointerId);
        edge = e.currentTarget;
        edge.addEventListener('pointermove', move);
      }}
      onPointerUp={stop}
      onPointerCancel={stop}
      onLostPointerCapture={stop}
      onKeyDown={(e) => {
        const grow = props.side === 'left' ? 'ArrowRight' : 'ArrowLeft';
        const shrink = props.side === 'left' ? 'ArrowLeft' : 'ArrowRight';
        let w: number;
        if (e.key === grow) w = clampPanel(props.width + 8);
        else if (e.key === shrink) w = clampPanel(props.width - 8);
        else return;
        e.preventDefault();
        ed.scheduler.mutate(() => props.onWidth(w));
      }}
    />
  );
}

/** Контекстное меню холста: команды редактора; доступность — из состояния
 * команд ядра (основной поток о выделении ничего не вычисляет). */
export function canvasMenu(): MenuItem[] {
  const item = (label: string, command: string): MenuItem => ({ label, command });
  return [
    item('Копировать', 'edit.copy'),
    item('Вырезать', 'edit.cut'),
    item('Вставить', 'edit.paste'),
    item('Дублировать', 'edit.duplicate'),
    item('Удалить', 'edit.delete'),
    { separator: true },
    item('Сгруппировать', 'object.group'),
    item('Разгруппировать', 'object.ungroup'),
    item('Обернуть во фрейм', 'object.frame_selection'),
    { separator: true },
    item('На передний план', 'arrange.front'),
    item('Переместить вперёд', 'arrange.forward'),
    item('Переместить назад', 'arrange.backward'),
    item('На задний план', 'arrange.back'),
    { separator: true },
    item('Отразить по горизонтали', 'object.flip_horizontal'),
    item('Отразить по вертикали', 'object.flip_vertical'),
    item('Скрыть или показать', 'object.toggle_visible'),
    item('Заблокировать или разблокировать', 'object.toggle_lock'),
    { separator: true },
    item('Копировать как PNG', 'edit.copy_png'),
    item('Копировать как SVG', 'edit.copy_svg'),
    item('Выделить всё', 'edit.select_all'),
    item('Показать всё', 'view.fit_all'),
  ];
}

export function Section(props: { title: string; children: JSX.Element; actions?: JSX.Element; muted?: boolean }) {
  return (
    <section class="fe-section" aria-label={props.title}>
      <div class="fe-section__head" classList={{ 't-muted': props.muted }}>
        <h2 class="fe-section__title">{props.title}</h2>
        <div class="fe-section__actions">{props.actions}</div>
      </div>
      {props.children}
    </section>
  );
}
