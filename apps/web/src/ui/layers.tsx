// Панель слоёв: виртуальный список. Основной поток не держит дерево — он просит
// у worker'а окно строк, которое видно (с запасом `OVERSCAN` сверху и снизу), и
// рисует только их; высота прокрутки — по общему числу строк. Раскрытие, поиск,
// диапазон для Shift считает worker. Переименование на месте, перетаскивание для
// порядка и вложенности, кнопки «скрыть» и «заблокировать», наведение
// подсвечивает узел на холсте, выделение синхронно в обе стороны.

import { createEffect, createMemo, createSignal, Index, on, onCleanup, onMount, Show, untrack } from 'solid-js';
import type { DropPosition, LayerRow, NodeType } from '../wasm/ferrum_wasm';
import { MAX_NAME_CHARS } from '../core/limits';
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
  /** Номер строки и глубина в момент нажатия: цели внутри себя и потомки недопустимы. */
  index: number;
  depth: number;
  /** Строка выделена (при перетаскивании невыделенной она выделяется сразу). */
  selected: boolean;
  /** Кого переносить — взято при нажатии: невыделенная строка — она сама,
   * выделенная — всё выделение по id, а если оно больше предела id — по версии
   * выделения (ядро откажет, если выделение с тех пор сменилось). */
  ids: string[] | undefined;
  version: number | undefined;
  y0: number;
  active: boolean;
  target: { index: number; position: DropPosition } | null;
}

export function LayersPanel() {
  const ed = useEditor();
  const layers = () => ed.store.state.layers;
  /** Открытое переименование: слой и имя на момент начала (применяется по id). */
  const [renaming, setRenaming] = createSignal<{ id: string; original: string } | null>(null);
  const [denied, setDenied] = createSignal(false);
  const [focusedSig, setFocusedSig] = createSignal<string | null>(null);
  const [topRow, setTopRow] = createSignal(0);
  /** Поле имени фокусируется только если переименование начал пользователь только что. */
  let wantFocus = false;
  let hovered = false;
  let search!: HTMLInputElement;
  const [drop, setDrop] = createSignal<{ index: number; position: DropPosition } | null>(null);
  let tree!: HTMLDivElement;
  let asked = { start: -1, count: -1 };
  let drag: Drag | null = null;
  /** Прокрутка и верх списка с последнего замера (для перетаскивания строк). */
  let scrollTop = 0;
  let treeTop: number | null = null;
  /** Строка с фокусом клавиатуры: DOM-строки переиспользуются по месту, и после
   * прокрутки фокус возвращается к своему слою, а не к месту. */
  /** Номер строки, куда перевести фокус, когда она придёт от worker'а. */
  let pendingFocus: number | null = null;

  /** Единственная точка Tab: строка с фокусом, иначе первая выделенная в окне,
   * иначе первая видимая (не запас). */
  const tabStop = createMemo(() => {
    const rows = layers().rows;
    const f = focusedSig();
    if (f && rows.some((r) => r.id === f)) return f;
    const sel = rows.find((r) => r.selected);
    if (sel) return sel.id;
    return (rows[topRow() - layers().start] ?? rows[0])?.id;
  });

  /** Чтение прокрутки — в фазе measure; запрос окна — только если оно сменилось. */
  let windowQueued = false;
  const requestWindow = () => {
    // Сколько бы событий прокрутки ни пришло, замер за кадр — один.
    if (windowQueued) return;
    windowQueued = true;
    ed.scheduler.measure(() => {
      windowQueued = false;
      scrollTop = tree.scrollTop;
      setTopRow(Math.floor(tree.scrollTop / ROW));
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

  // Другая страница или новый документ: worker сбросил поиск, поле и прокрутка — тоже.
  createEffect(
    on(
      () => [ed.store.state.pages.current, ed.store.state.docSeq],
      () => {
        setRenaming(null);
        setFocusedSig(null);
        ed.scheduler.mutate(() => {
          search.value = '';
          tree.scrollTop = 0;
          requestWindow();
        });
      },
      { defer: true },
    ),
  );

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
        if (pendingFocus != null && pendingFocus >= layers().total) pendingFocus = null;
        if (pendingFocus != null) {
          const el = tree.querySelector<HTMLElement>(`[data-index="${pendingFocus}"]`);
          if (el) {
            pendingFocus = null;
            el.focus();
          }
          return;
        }
        const active = document.activeElement as HTMLElement | null;
        const fid = focusedSig();
        if (!fid || !active || !tree.contains(active) || active.dataset.id === fid) return;
        const own = tree.querySelector<HTMLElement>(`[data-id="${CSS.escape(fid)}"]`);
        if (own) own.focus({ preventScroll: true });
        else tree.focus({ preventScroll: true });
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

  const endDrag = () => {
    drag = null;
    setDrop(null);
    setDenied(false);
    window.removeEventListener('keydown', onDragKey, true);
  };
  /** Во время перетаскивания клавиши не действуют (Ctrl+A, Delete сменили бы, что
   * переносится); Esc отменяет жест — без переноса и шага. Ядру ничего не уходит. */
  function onDragKey(e: KeyboardEvent) {
    if (!drag) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') endDrag();
  }

  const onPointerDown = (row: LayerRow, index: number, e: PointerEvent) => {
    if (e.button !== 0 || renaming()?.id === row.id) return;
    const el = e.currentTarget as HTMLElement;
    const st = ed.store.state;
    const capped = st.selectionCount > st.selectionIds.length;
    drag = {
      id: row.id,
      index,
      depth: row.depth,
      selected: row.selected,
      ids: !row.selected ? [row.id] : capped ? undefined : [...st.selectionIds],
      version: row.selected && capped ? st.selectionVersion : undefined,
      y0: e.clientY,
      active: false,
      target: null,
    };
    el.setPointerCapture(e.pointerId);
    requestWindow();
  };

  /** Цель недопустима: сама строка, её потомки (идут следом и глубже), другая переносимая. */
  const invalidTarget = (d: Drag, index: number, row: LayerRow): boolean => {
    if (index === d.index || (d.selected && row.selected)) return true;
    if (index < d.index) return false;
    for (let k = Math.max(d.index + 1, layers().start); k <= index; k++) {
      const r = layers().rows[k - layers().start];
      if (!r || r.depth <= d.depth) return false;
    }
    return true;
  };

  const onPointerMove = ed.core.costed((e: PointerEvent) => {
    if (!drag || treeTop === null) return;
    if (!drag.active && Math.abs(e.clientY - drag.y0) < DRAG_START_PX) return;
    if (!drag.active) {
      drag.active = true;
      window.addEventListener('keydown', onDragKey, true);
      // Как в Figma: невыделенная строка выделяется и переносится одна.
      if (!drag.selected) {
        ed.send({ type: 'select', ids: [drag.id], mode: 'replace' });
      }
    }
    // Прокрутка во время перетаскивания обновляет `scrollTop` (замер окна).
    const y = e.clientY - treeTop + scrollTop;
    const index = Math.floor(y / ROW);
    const row = layers().rows[index - layers().start];
    const bad = !!row && invalidTarget(drag, index, row);
    drag.target = row && !bad ? { index, position: dropPosition(y - index * ROW, row) } : null;
    setDenied(bad);
    setDrop(drag.target);
  });

  const onPointerUp = (row: LayerRow, e: PointerEvent) => {
    const d = drag;
    endDrag();
    if (!d) return;
    if (!d.active) {
      select(row, e);
      return;
    }
    const target = d.target && layers().rows[d.target.index - layers().start];
    if (!target) return;
    // Переносится то, что взято при нажатии (см. `Drag.ids`), а не выделение на
    // момент броска.
    ed.send({ type: 'moveLayers', ids: d.ids, selectionVersion: d.version, target: target.id, position: d.target!.position });
  };

  const startRename = (row: LayerRow) => {
    wantFocus = true;
    setRenaming({ id: row.id, original: row.name });
  };

  /** Применить имя слоя по id (поле могло переехать на другую строку окна). */
  const commitRename = (id: string, name: string) => {
    const r = renaming();
    // Esc или уже применено: удаляемое поле ничего не применяет.
    if (!r || r.id !== id) return;
    setRenaming(null);
    if (name !== r.original) ed.send({ type: 'renameLayer', id, name });
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
        startRename(row);
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
          ref={search}
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
        aria-multiselectable="true"
        classList={{ 'fe-tree--denied': denied() }}
        onScroll={ed.core.costed(() => {
          // Строки под указателем сменились: старая подсветка на холсте неверна.
          if (hovered) {
            hovered = false;
            ed.send({ type: 'hoverLayer', id: undefined });
          }
          requestWindow();
        })}
        onFocusOut={(e) => {
          const to = e.relatedTarget as Node | null;
          if (to && !tree.contains(to)) setFocusedSig(null);
        }}
        onPointerLeave={() => {
          hovered = false;
          ed.send({ type: 'hoverLayer', id: undefined });
        }}
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
                tabindex={row().id === tabStop() ? 0 : -1}
                title={row().name}
                onPointerEnter={() => {
                  hovered = true;
                  ed.send({ type: 'hoverLayer', id: row().id });
                }}
                onPointerDown={(e) => onPointerDown(row(), index(), e)}
                onPointerMove={onPointerMove}
                onPointerUp={(e) => onPointerUp(row(), e)}
                onPointerCancel={endDrag}
                onFocus={() => setFocusedSig(row().id)}
                onDblClick={() => startRename(row())}
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
                <Show when={renaming()?.id === row().id} fallback={<span class="fe-layer__name">{row().name}</span>}>
                  {(() => {
                    // Поле принадлежит слою, а не месту: исчезло из окна — применяется по id.
                    const id = row().id;
                    let el!: HTMLInputElement;
                    const focus = wantFocus;
                    wantFocus = false;
                    onCleanup(() => commitRename(id, el.value));
                    if (focus)
                      queueMicrotask(() => {
                        el.focus();
                        el.select();
                      });
                    return (
                      <input
                        ref={el}
                        class="fe-rename fe-layer__rename"
                        aria-label="Имя слоя"
                        maxlength={MAX_NAME_CHARS}
                        // Набранное поле не перезаписывается событиями: имя берётся один раз.
                        value={untrack(() => row().name)}
                        onPointerDown={(e) => e.stopPropagation()}
                        onKeyDown={(e) => {
                          e.stopPropagation();
                          if (e.key === 'Enter') {
                            commitRename(id, e.currentTarget.value);
                            refocusRow(id);
                          }
                          if (e.key === 'Escape') {
                            setRenaming(null);
                            refocusRow(id);
                          }
                        }}
                        onBlur={(e) => commitRename(id, e.currentTarget.value)}
                      />
                    );
                  })()}
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
