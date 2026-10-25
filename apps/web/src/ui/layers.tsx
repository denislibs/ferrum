// Панель слоёв: виртуальный список. Основной поток не держит дерево — он просит
// у worker'а окно строк, которое видно (с запасом `OVERSCAN` сверху и снизу), и
// рисует только их; высота прокрутки — по общему числу строк. Раскрытие, поиск,
// диапазон для Shift считает worker. Переименование на месте, перетаскивание для
// порядка и вложенности, кнопки «скрыть» и «заблокировать», наведение
// подсвечивает узел на холсте, выделение синхронно в обе стороны.

import { createEffect, createSignal, Index, on, onCleanup, onMount, Show } from 'solid-js';
import type { DropPosition, LayerRow, NodeType } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Icon, type IconName } from './icons';

/** Высота строки — `row-height` дизайн-системы. */
export const ROW = 28;
/** Строк сверху и снизу от видимых: прокрутка не показывает пустоты. */
export const OVERSCAN = 10;
/** Сдвиг указателя, после которого нажатие на строке становится перетаскиванием. */
const DRAG_START_PX = 4;

const KIND_ICON: Record<NodeType, IconName> = {
  frame: 'frame',
  group: 'group',
  rect: 'rect',
  ellipse: 'ellipse',
  line: 'line',
  image: 'image',
};

/** Окно строк для прокрутки `scrollTop` и высоты `height`. */
export function windowFor(scrollTop: number, height: number): { start: number; count: number } {
  const first = Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN);
  return { start: first, count: Math.ceil(height / ROW) + 2 * OVERSCAN };
}

/** Куда бросить: верхняя четверть строки — над ней, нижняя — под ней, середина
 * контейнера — внутрь; у листа середина делится пополам. */
export function dropPosition(offset: number, row: Pick<LayerRow, 'kind'>): DropPosition {
  const container = row.kind === 'frame' || row.kind === 'group';
  if (container) {
    if (offset < ROW / 4) return 'above';
    if (offset > (ROW * 3) / 4) return 'below';
    return 'into';
  }
  return offset < ROW / 2 ? 'above' : 'below';
}

interface Drag {
  id: string;
  selected: boolean;
  y0: number;
  active: boolean;
  target: { index: number; position: DropPosition } | null;
}

export function LayersPanel() {
  const ed = useEditor();
  const layers = () => ed.store.state.layers;
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [drop, setDrop] = createSignal<{ index: number; position: DropPosition } | null>(null);
  let tree!: HTMLDivElement;
  let asked = { start: -1, count: -1 };
  let drag: Drag | null = null;
  /** Прокрутка и верх списка с последнего замера (для перетаскивания строк). */
  let scrollTop = 0;
  let treeTop: number | null = null;
  /** Строка с фокусом клавиатуры: DOM-строки переиспользуются по месту, и после
   * прокрутки фокус возвращается к своему слою, а не к месту. */
  let focusedId: string | null = null;
  /** Номер строки, куда перевести фокус, когда она придёт от worker'а. */
  let pendingFocus: number | null = null;

  /** Чтение прокрутки — в фазе measure; запрос окна — только если оно сменилось. */
  let windowQueued = false;
  const requestWindow = () => {
    // Сколько бы событий прокрутки ни пришло, замер за кадр — один.
    if (windowQueued) return;
    windowQueued = true;
    ed.scheduler.measure(() => {
      windowQueued = false;
      scrollTop = tree.scrollTop;
      treeTop = tree.getBoundingClientRect().top;
      const w = windowFor(tree.scrollTop, tree.clientHeight);
      if (w.start !== asked.start || w.count !== asked.count) {
        asked = w;
        ed.send({ type: 'layersWindow', start: w.start, count: w.count });
      }
    });
  };

  onMount(() => {
    requestWindow();
    const ro = new ResizeObserver(() => requestWindow());
    ro.observe(tree);
    onCleanup(() => ro.disconnect());
  });

  // Выделение пришло с холста: worker раскрыл предков и прислал строку — прокрутить
  // к ней, если её не видно.
  createEffect(
    on(
      () => layers().revealSeq,
      () => {
        const at = layers().reveal;
        if (at == null) return;
        ed.scheduler.measure(() => {
          const top = at * ROW;
          if (top < tree.scrollTop || top + ROW > tree.scrollTop + tree.clientHeight) {
            const next = Math.max(0, top - tree.clientHeight / 2);
            ed.scheduler.mutate(() => {
              tree.scrollTop = next;
              requestWindow();
            });
          }
        });
      },
      { defer: true },
    ),
  );

  // Новые строки окна: фокус — к своему слою или к строке, которую ждали.
  createEffect(
    on(
      () => [layers().start, layers().rows.map((r) => r.id).join()],
      () => {
        if (pendingFocus != null) {
          const el = tree.querySelector<HTMLElement>(`[data-index="${pendingFocus}"]`);
          if (el) {
            pendingFocus = null;
            el.focus();
          }
          return;
        }
        const active = document.activeElement as HTMLElement | null;
        if (!focusedId || !active || !tree.contains(active) || active.dataset.id === focusedId) return;
        const own = tree.querySelector<HTMLElement>(`[data-id="${CSS.escape(focusedId)}"]`);
        if (own) own.focus();
        else tree.focus();
      },
      { defer: true },
    ),
  );

  /** Фокус на строку `index`: сразу, если строка в DOM, иначе — когда придёт от
   * worker'а; список подкручивается к ней в фазах кадра. */
  const focusAt = (index: number) => {
    const i = Math.max(0, Math.min(layers().total - 1, index));
    const el = tree.querySelector<HTMLElement>(`[data-index="${i}"]`);
    pendingFocus = el ? null : i;
    el?.focus({ preventScroll: true });
    ed.scheduler.measure(() => {
      const top = i * ROW;
      const next =
        top < tree.scrollTop ? top : top + ROW > tree.scrollTop + tree.clientHeight ? top + ROW - tree.clientHeight : null;
      if (next == null) return;
      ed.scheduler.mutate(() => {
        tree.scrollTop = next;
        requestWindow();
      });
    });
  };

  const select = (row: LayerRow, e: MouseEvent) => {
    const mode = e.shiftKey ? 'range' : e.metaKey || e.ctrlKey ? 'toggle' : 'replace';
    ed.send({ type: 'select', ids: [row.id], mode });
  };

  const onPointerDown = (row: LayerRow, e: PointerEvent) => {
    if (e.button !== 0 || renaming()) return;
    const el = e.currentTarget as HTMLElement;
    drag = { id: row.id, selected: row.selected, y0: e.clientY, active: false, target: null };
    el.setPointerCapture(e.pointerId);
    requestWindow();
  };

  const onPointerMove = ed.core.costed((e: PointerEvent) => {
    if (!drag || treeTop === null) return;
    if (!drag.active && Math.abs(e.clientY - drag.y0) < DRAG_START_PX) return;
    drag.active = true;
    // Прокрутка во время перетаскивания обновляет `scrollTop` (замер окна).
    const y = e.clientY - treeTop + scrollTop;
    const index = Math.floor(y / ROW);
    const row = layers().rows[index - layers().start];
    drag.target = row && row.id !== drag.id ? { index, position: dropPosition(y - index * ROW, row) } : null;
    setDrop(drag.target);
  });

  const onPointerUp = (row: LayerRow, e: PointerEvent) => {
    const d = drag;
    drag = null;
    setDrop(null);
    if (!d) return;
    if (!d.active) {
      select(row, e);
      return;
    }
    const target = d.target && layers().rows[d.target.index - layers().start];
    if (!target) return;
    // Перетащили выделенную строку — переносится всё выделение (его полный список
    // есть только у worker'а).
    ed.send({
      type: 'moveLayers',
      ids: d.selected ? undefined : [d.id],
      target: target.id,
      position: d.target!.position,
    });
  };

  const rename = (row: LayerRow, name: string) => {
    // Esc уже закрыл поле: blur удаляемого поля не должен применять набранное.
    if (renaming() !== row.id) return;
    setRenaming(null);
    if (name !== row.name) ed.send({ type: 'renameLayer', id: row.id, name });
  };

  /** После Enter и Esc поле имени исчезает вместе с фокусом: он возвращается к строке. */
  const refocusRow = (id: string) =>
    queueMicrotask(() => tree.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`)?.focus({ preventScroll: true }));

  const onKey = (row: LayerRow, index: number, e: KeyboardEvent) => {
    switch (e.key) {
      // Пока ждём строку от worker'а, следующие нажатия считаются от неё.
      case 'ArrowDown':
        focusAt((pendingFocus ?? index) + 1);
        break;
      case 'ArrowUp':
        focusAt((pendingFocus ?? index) - 1);
        break;
      case 'ArrowRight':
        if (row.hasChildren && !row.expanded) ed.send({ type: 'expandLayer', id: row.id, expanded: true });
        break;
      case 'ArrowLeft':
        if (row.expanded) ed.send({ type: 'expandLayer', id: row.id, expanded: false });
        break;
      case ' ':
        ed.send({ type: 'select', ids: [row.id], mode: 'replace' });
        break;
      case 'Enter':
      case 'F2':
        setRenaming(row.id);
        break;
      // Та же команда, что на холсте; ядру клавишу не отдаём (preventDefault ниже).
      case 'Delete':
      case 'Backspace':
        ed.run('edit.delete');
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <section class="fe-layers-panel" aria-label="Слои">
      <div class="fe-section__head fe-layers-panel__head">
        <h2 class="fe-section__title">Слои</h2>
      </div>
      <label class="fe-field fe-field--text fe-layers-panel__search">
        <Icon name="search" class="t-muted fe-field__icon" />
        <input
          type="search"
          placeholder="Поиск слоёв"
          aria-label="Поиск слоёв"
          // Кэш имён строится заранее, пока пользователь ещё набирает запрос.
          onFocus={() => ed.send({ type: 'prepareSearch' })}
          onInput={(e) => ed.send({ type: 'filterLayers', query: e.currentTarget.value })}
        />
      </label>
      <div
        ref={tree}
        class="fe-tree"
        role="tree"
        tabindex="-1"
        aria-label="Слои"
        onScroll={ed.core.costed(() => requestWindow())}
        onPointerLeave={() => ed.send({ type: 'hoverLayer', id: undefined })}
      >
        <div class="fe-tree__spacer" style={{ height: `${layers().total * ROW}px` }} />
        {/* Index, а не For: при прокрутке строки DOM переиспользуются по месту —
            меняются текст и атрибуты, а не создаются узлы с иконками заново. */}
        <Index each={layers().rows}>
          {(row, i) => {
            const index = () => layers().start + i;
            return (
              <div
                class="fe-layer"
                classList={{
                  'fe-layer--child-of-selected': row().parentSelected && !row().selected,
                  'fe-layer--hidden': !row().visible || row().ancestorHidden,
                  'fe-layer--drop-into': drop()?.index === index() && drop()?.position === 'into',
                  'fe-layer--drop-above': drop()?.index === index() && drop()?.position === 'above',
                  'fe-layer--drop-below': drop()?.index === index() && drop()?.position === 'below',
                }}
                style={{ top: `${index() * ROW}px`, 'padding-left': `${row().depth * 12 + 4}px` }}
                role="treeitem"
                aria-label={row().name}
                data-id={row().id}
                data-index={index()}
                aria-level={row().depth + 1}
                aria-selected={row().selected}
                aria-expanded={row().hasChildren ? row().expanded : undefined}
                tabindex={row().selected || (index() === layers().start && !layers().rows.some((r) => r.selected)) ? 0 : -1}
                title={row().name}
                onPointerEnter={() => ed.send({ type: 'hoverLayer', id: row().id })}
                onPointerDown={(e) => onPointerDown(row(), e)}
                onPointerMove={onPointerMove}
                onPointerUp={(e) => onPointerUp(row(), e)}
                onPointerCancel={() => {
                  drag = null;
                  setDrop(null);
                }}
                onFocus={() => (focusedId = row().id)}
                onDblClick={() => setRenaming(row().id)}
                onKeyDown={(e) => onKey(row(), index(), e)}
              >
                <span class="fe-layer__caret">
                  <Show when={row().hasChildren}>
                    <button
                      class="fe-layer__toggle"
                      classList={{ 'fe-layer__toggle--open': row().expanded }}
                      tabindex="-1"
                      aria-label={row().expanded ? 'Свернуть' : 'Развернуть'}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={(e) => {
                        e.stopPropagation();
                        ed.send({ type: 'expandLayer', id: row().id, expanded: !row().expanded });
                      }}
                    >
                      <Icon name="expand" size={12} />
                    </button>
                  </Show>
                </span>
                <span class="fe-layer__icon">
                  <Icon name={KIND_ICON[row().kind]} />
                </span>
                <Show when={renaming() === row().id} fallback={<span class="fe-layer__name">{row().name}</span>}>
                  <input
                    class="fe-rename fe-layer__rename"
                    aria-label="Имя слоя"
                    value={row().name}
                    ref={(el) =>
                      queueMicrotask(() => {
                        el.focus();
                        el.select();
                      })
                    }
                    onPointerDown={(e) => e.stopPropagation()}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === 'Enter') {
                        rename(row(), e.currentTarget.value);
                        refocusRow(row().id);
                      }
                      if (e.key === 'Escape') {
                        setRenaming(null);
                        refocusRow(row().id);
                      }
                    }}
                    onBlur={(e) => rename(row(), e.currentTarget.value)}
                  />
                </Show>
                <span class="fe-layer__actions">
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    classList={{ 'fe-layer__flag--on': row().locked }}
                    tabindex="-1"
                    aria-label={row().locked ? 'Разблокировать' : 'Заблокировать'}
                    aria-pressed={row().locked}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      ed.send({ type: 'setLayerFlags', ids: [row().id], visible: undefined, locked: !row().locked });
                    }}
                  >
                    <Icon name={row().locked ? 'lock' : 'unlock'} />
                  </button>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    classList={{ 'fe-layer__flag--on': !row().visible }}
                    tabindex="-1"
                    aria-label={row().visible ? 'Скрыть' : 'Показать'}
                    aria-pressed={!row().visible}
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      ed.send({ type: 'setLayerFlags', ids: [row().id], visible: !row().visible, locked: undefined });
                    }}
                  >
                    <Icon name={row().visible ? 'eye' : 'eyeOff'} />
                  </button>
                </span>
              </div>
            );
          }}
        </Index>
      </div>
    </section>
  );
}
