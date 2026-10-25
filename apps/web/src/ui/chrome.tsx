// Шапки панелей (PanelHeader), меню файла, зум, «Поделиться», вкладки, края
// панелей для изменения ширины и контекстное меню холста.

import { createSignal, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem } from './menu';
import { roving } from './roving';

/** Пределы ширины боковых панелей. */
export const PANEL_MIN = 200;
export const PANEL_MAX = 480;

export function clampPanel(w: number): number {
  return Math.round(Math.min(PANEL_MAX, Math.max(PANEL_MIN, w)));
}

/** Пункты меню файла: другие части интерфейса добавляют свои (открыть, сохранить,
 * настройки). */
export function FileHeader(props: { items: () => MenuItem[] }) {
  const ed = useEditor();
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const [editing, setEditing] = createSignal(false);
  const commit = (value: string) => {
    // Esc уже закрыл поле: blur удаляемого поля не должен применять набранное.
    if (!editing()) return;
    setEditing(false);
    if (value.trim() && value !== ed.store.state.documentName) ed.send({ type: 'setDocumentName', name: value });
  };
  return (
    <div class="fe-panel-head">
      <span class="fe-panel-head__mark" aria-hidden="true" />
      <div class="fe-panel-head__file">
        <Show
          when={editing()}
          fallback={
            <span
              class="fe-panel-head__name"
              title={ed.store.state.documentName}
              onDblClick={() => setEditing(true)}
            >
              {ed.store.state.documentName}
            </span>
          }
        >
          <input
            class="fe-rename"
            aria-label="Имя файла"
            value={ed.store.state.documentName}
            ref={(el) => queueMicrotask(() => el.select())}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commit(e.currentTarget.value);
              if (e.key === 'Escape') setEditing(false);
            }}
            onBlur={(e) => commit(e.currentTarget.value)}
          />
        </Show>
        <span class="fe-panel-head__project">Черновики</span>
      </div>
      <button
        class="fe-tool fe-tool--small"
        aria-haspopup="menu"
        aria-expanded={!!menu()}
        aria-label="Меню файла"
        data-tip="Меню файла"
        onClick={(e) => setMenu(e.currentTarget)}
      >
        <Caret />
      </button>
      <Show when={menu()}>
        {(a) => <Menu label="Меню файла" items={props.items()} anchor={a()} onClose={() => setMenu(null)} />}
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
    { label: 'Показать выделение', command: 'view.fit_selection', disabled: ed.store.state.selectionCount === 0 },
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
  const move = ed.core.costed((e: PointerEvent) => {
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
        e.currentTarget.addEventListener('pointermove', move);
      }}
      onPointerUp={(e) => e.currentTarget.removeEventListener('pointermove', move)}
      onPointerCancel={(e) => e.currentTarget.removeEventListener('pointermove', move)}
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

/** Контекстное меню холста: команды редактора по выделению. */
export function canvasMenu(selection: number): MenuItem[] {
  const none = selection === 0;
  const item = (label: string, command: string, needs = true): MenuItem => ({ label, command, disabled: needs && none });
  return [
    item('Копировать', 'edit.copy'),
    item('Вырезать', 'edit.cut'),
    item('Вставить', 'edit.paste', false),
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
    item('Выделить всё', 'edit.select_all', false),
    item('Показать всё', 'view.fit_all', false),
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
