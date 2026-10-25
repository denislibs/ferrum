// Секции «Заливка» и «Обводка» и фон страницы. Списки заливок и обводок
// приходят в сводке (у всех выделенных одинаковые — список, разные —
// «Смешанные»); любая правка шлёт новый список целиком (`editSelection` →
// `fills`/`strokes`), палитра — превью во время жеста и один шаг при отпускании.
// Заливка изображением показывается, но не правится (картинку меняет этап 6).
//
// Правка строится от списка и выделения на её начало (`begin`): жест палитры или
// набор в поле не должны попасть в тех, кого выделили позже, и не должны
// переписать их список. Палитра закрывается, когда выделение сменилось.

import { createEffect, createSignal, For, Index, on, Show, untrack } from 'solid-js';
import type { Align, EditPhase, FillDto, StrokeDto } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { cssColor, paintOpacity, parseHex, parseOpacityText, toHex, toHex6, type Rgb } from './colour';
import { CommitInput } from './commit-input';
import { useEditor } from './context';
import { Icon } from './icons';
import { formatNumber } from './number';
import { captureEdit, NumberField, type EditCapture } from './number-field';
import { ColorPicker } from './picker';
import { sendEdit } from './properties';

/** Цвет новой заливки (как у новой фигуры) и обводки. */
const NEW_FILL: FillDto = { kind: 'solid', color: '#d9d9d9ff', opacity: 1, visible: true, imageHash: undefined, scaleMode: undefined };
const NEW_STROKE: StrokeDto = { color: '#000000ff', width: 1, align: 'inside' };

const ALIGNS: { value: Align; label: string }[] = [
  { value: 'inside', label: 'Внутри' },
  { value: 'center', label: 'По центру' },
  { value: 'outside', label: 'Снаружи' },
];

const SCALE_MODES = { fill: 'Заполнить', fit: 'Вписать', tile: 'Плитка' } as const;

/** Цвет из `#rrggbbaa` и прозрачность для панели. */
function shown(color: string, opacity: number): { rgb: Rgb & { a: number }; alpha: number } {
  const c = parseHex(color) ?? { r: 0, g: 0, b: 0, a: 255 };
  return { rgb: c, alpha: paintOpacity(c, opacity) };
}

/** Выделение и список заливок (или обводок) на начало правки. Элементы — обычные
 * копии: через границу потоков уходит не прокси хранилища. */
type FillBase = EditCapture & { list: FillDto[] };
type StrokeBase = EditCapture & { list: StrokeDto[] };

/** Палитра и список закрываются вместе с выделением, к которому открыты. */
function closeOnSelection(ed: ReturnType<typeof useEditor>, open: () => boolean, close: () => void) {
  let at: EditCapture | null = null;
  createEffect(() => {
    const o = open();
    at = o ? (at ?? untrack(() => captureEdit(ed))) : null;
  });
  createEffect(
    on(
      () => [ed.store.state.selectionIds, ed.store.state.selectionVersion],
      () => {
        if (at?.changed()) close();
      },
      { defer: true },
    ),
  );
}

export function FillsSection() {
  const ed = useEditor();
  const paints = () => ed.store.state.summary?.fills;
  const [picker, setPicker] = createSignal<{ index: number; anchor: HTMLElement } | null>(null);
  const list = () => (paints()?.list ?? []) as FillDto[];
  const begin = (): FillBase => ({ ...captureEdit(ed), list: list().map((x) => ({ ...x })) });
  const send = (b: FillBase, fills: FillDto[], phase: EditPhase) => sendEdit(ed.send, { type: 'fills', fills }, phase, b.target);
  const replace = (b: FillBase, i: number, f: Partial<FillDto>, phase: EditPhase = 'commit') =>
    send(b, b.list.map((x, j) => (j === i ? { ...x, ...f } : x)), phase);
  closeOnSelection(ed, () => !!picker(), () => setPicker(null));
  // Список стал короче, чем индекс открытой палитры (удалили заливку).
  createEffect(() => {
    const pk = picker();
    if (pk && !list()[pk.index]) setPicker(null);
  });
  return (
    <Show when={paints()}>
      {(p) => (
        <Section
          title="Заливка"
          muted={!p().mixed && p().list.length === 0}
          actions={
            <button
              class="fe-tool fe-tool--small"
              aria-label="Добавить заливку"
              data-tip="Добавить заливку"
              onClick={() => {
                const b = begin();
                send(b, p().mixed ? [NEW_FILL] : [...b.list, NEW_FILL], 'commit');
              }}
            >
              <Icon name="plus" />
            </button>
          }
        >
          <Show when={p().mixed}>
            <p class="fe-paint__mixed">Смешанные заливки — нажмите «+», чтобы заменить</p>
          </Show>
          {/* Верхняя заливка — первой, как в Figma: список документа идёт снизу вверх. */}
          <Index each={[...list()].map((_, i) => list().length - 1 - i)}>
            {(index) => {
              const fill = () => list()[index()];
              const c = () => shown(fill().color, fill().opacity);
              return (
                <div class="fe-paint" classList={{ 'fe-paint--hidden': !fill().visible }}>
                  <Show
                    when={fill().kind === 'solid'}
                    fallback={
                      <>
                        <span class="fe-swatch fe-swatch--image" aria-hidden="true">
                          <Icon name="image" />
                        </span>
                        <span class="fe-row__grow fe-paint__label">
                          Изображение · {SCALE_MODES[fill().scaleMode ?? 'fill']}
                        </span>
                      </>
                    }
                  >
                    <button
                      class="fe-swatch"
                      aria-label="Цвет заливки"
                      aria-haspopup="dialog"
                      aria-expanded={picker()?.index === index()}
                      style={{ '--swatch': cssColor(c().rgb, c().alpha) }}
                      onClick={(e) =>
                        setPicker(picker()?.index === index() ? null : { index: index(), anchor: e.currentTarget })
                      }
                    />
                    <CommitInput
                      name="HEX заливки"
                      class="fe-row__grow"
                      text={toHex6(c().rgb)}
                      maxLength={10}
                      capture={begin}
                      onCommit={(text, b) => {
                        const typed = parseHex(text);
                        const now = b.list[index()];
                        if (!typed || !now) return;
                        const cur = shown(now.color, now.opacity);
                        if (toHex6(typed) === toHex6(cur.rgb)) return;
                        replace(b, index(), { color: toHex({ ...typed, a: 255 }), opacity: cur.alpha });
                      }}
                    />
                    <CommitInput
                      name="Прозрачность заливки"
                      class="fe-paint__opacity"
                      text={`${formatNumber(c().alpha * 100, 0)}%`}
                      maxLength={16}
                      capture={begin}
                      onCommit={(text, b) => {
                        const now = b.list[index()];
                        if (!now) return;
                        const cur = shown(now.color, now.opacity);
                        const v = parseOpacityText(text, cur.alpha * 100);
                        if (v === null || Math.round(v * 100) === Math.round(cur.alpha * 100)) return;
                        replace(b, index(), { color: toHex({ ...cur.rgb, a: 255 }), opacity: v });
                      }}
                    />
                  </Show>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    aria-label={fill().visible ? 'Скрыть заливку' : 'Показать заливку'}
                    aria-pressed={!fill().visible}
                    onClick={() => {
                      const b = begin();
                      replace(b, index(), { visible: !b.list[index()].visible });
                    }}
                  >
                    <Icon name={fill().visible ? 'eye' : 'eyeOff'} />
                  </button>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted"
                    aria-label="Удалить заливку"
                    onClick={() => {
                      const b = begin();
                      send(b, b.list.filter((_, j) => j !== index()), 'commit');
                    }}
                  >
                    <Icon name="minus" />
                  </button>
                </div>
              );
            }}
          </Index>
          <Show when={picker()}>
            {(pk) => {
              // Индекс — один раз: после закрытия палитры доступ к `pk()` бросает, а отмена жеста
              // при закрытии ещё должна уйти.
              const index = pk().index;
              const now = list()[index];
              const start = shown(now.color, now.opacity);
              return (
                <ColorPicker
                  title="Цвет заливки"
                  color={start.rgb}
                  alpha={start.alpha}
                  anchor={pk().anchor}
                  begin={begin}
                  onChange={(rgb, alpha, phase, b) =>
                    replace(b, index, { color: toHex({ ...rgb, a: 255 }), opacity: alpha }, phase)
                  }
                  onClose={() => setPicker(null)}
                />
              );
            }}
          </Show>
        </Section>
      )}
    </Show>
  );
}

export function StrokesSection() {
  const ed = useEditor();
  const paints = () => ed.store.state.summary?.strokes;
  const [picker, setPicker] = createSignal<{ index: number; anchor: HTMLElement } | null>(null);
  const list = () => (paints()?.list ?? []) as StrokeDto[];
  const begin = (): StrokeBase => ({ ...captureEdit(ed), list: list().map((x) => ({ ...x })) });
  const send = (b: StrokeBase, strokes: StrokeDto[], phase: EditPhase) =>
    sendEdit(ed.send, { type: 'strokes', strokes }, phase, b.target);
  const replace = (b: StrokeBase, i: number, s: Partial<StrokeDto>, phase: EditPhase = 'commit') =>
    send(b, b.list.map((x, j) => (j === i ? { ...x, ...s } : x)), phase);
  closeOnSelection(ed, () => !!picker(), () => setPicker(null));
  createEffect(() => {
    const pk = picker();
    if (pk && !list()[pk.index]) setPicker(null);
  });
  return (
    <Show when={paints()}>
      {(p) => (
        <Section
          title="Обводка"
          muted={!p().mixed && p().list.length === 0}
          actions={
            <button
              class="fe-tool fe-tool--small"
              aria-label="Добавить обводку"
              data-tip="Добавить обводку"
              onClick={() => {
                const b = begin();
                send(b, p().mixed ? [NEW_STROKE] : [...b.list, NEW_STROKE], 'commit');
              }}
            >
              <Icon name="plus" />
            </button>
          }
        >
          <Show when={p().mixed}>
            <p class="fe-paint__mixed">Смешанные обводки — нажмите «+», чтобы заменить</p>
          </Show>
          <For each={list()}>
            {(stroke, i) => {
              const c = () => shown(stroke.color, 1);
              return (
                <div class="fe-stroke">
                  <div class="fe-paint">
                    <button
                      class="fe-swatch"
                      aria-label="Цвет обводки"
                      aria-haspopup="dialog"
                      aria-expanded={picker()?.index === i()}
                      style={{ '--swatch': cssColor(c().rgb, c().alpha) }}
                      onClick={(e) => setPicker(picker()?.index === i() ? null : { index: i(), anchor: e.currentTarget })}
                    />
                    <CommitInput
                      name="HEX обводки"
                      class="fe-row__grow"
                      text={toHex6(c().rgb)}
                      maxLength={10}
                      capture={begin}
                      onCommit={(text, b) => {
                        const typed = parseHex(text);
                        const now = b.list[i()];
                        if (!typed || !now) return;
                        const cur = shown(now.color, 1);
                        if (toHex6(typed) === toHex6(cur.rgb)) return;
                        // Прозрачность обводки — в её цвете: правка HEX её не трогает.
                        replace(b, i(), { color: toHex({ ...typed, a: cur.rgb.a }) });
                      }}
                    />
                    <button
                      class="fe-tool fe-tool--small fe-tool--muted"
                      aria-label="Удалить обводку"
                      onClick={() => {
                        const b = begin();
                        send(b, b.list.filter((_, j) => j !== i()), 'commit');
                      }}
                    >
                      <Icon name="minus" />
                    </button>
                  </div>
                  <div class="fe-grid2">
                    <NumberField
                      label="W"
                      name="Толщина обводки"
                      value={stroke.width}
                      min={0}
                      capture={begin}
                      onEdit={(input, phase, target, b) => {
                        const now = b.list[i()];
                        if (!now) return;
                        const width = input.kind === 'set' ? input.value : now.width + input.delta;
                        sendEdit(
                          ed.send,
                          { type: 'strokes', strokes: b.list.map((x, j) => (j === i() ? { ...x, width } : x)) },
                          phase,
                          target,
                        );
                      }}
                    />
                    <label class="fe-field">
                      <select
                        class="fe-native-select"
                        aria-label="Выравнивание обводки"
                        value={stroke.align}
                        onChange={(e) => replace(begin(), i(), { align: e.currentTarget.value as Align })}
                      >
                        <For each={ALIGNS}>{(a) => <option value={a.value}>{a.label}</option>}</For>
                      </select>
                    </label>
                  </div>
                </div>
              );
            }}
          </For>
          <Show when={picker()}>
            {(pk) => {
              const index = pk().index;
              const start = shown(list()[index].color, 1);
              return (
                <ColorPicker
                  title="Цвет обводки"
                  color={start.rgb}
                  alpha={start.alpha}
                  anchor={pk().anchor}
                  begin={begin}
                  onChange={(rgb, alpha, phase, b) =>
                    replace(b, index, { color: toHex({ ...rgb, a: Math.round(alpha * 255) }) }, phase)
                  }
                  onClose={() => setPicker(null)}
                />
              );
            }}
          </Show>
        </Section>
      )}
    </Show>
  );
}

/** Холст темы, если у страницы свой фон не задан (токен `canvas`). */
export const THEME_CANVAS = { dark: '#1a1918ff', light: '#e9e7e4ff' } as const;

/** У фона страницы выделения нет: правка не адресована никому. */
const NO_SELECTION = { capped: false, changed: () => false };

/** Без выделения: фон страницы. В историю отмены не попадает (как и всё о
 * страницах); жест палитры — превью в кадре, в документ — при отпускании, Esc
 * возвращает фон документа. */
export function PageSection(props: { theme: 'dark' | 'light' }) {
  const ed = useEditor();
  const [picker, setPicker] = createSignal<HTMLElement | null>(null);
  const bg = () => ed.store.state.pages.background;
  const c = () => shown(bg() ?? THEME_CANVAS[props.theme], 1);
  const set = (rgb: Rgb, alpha: number, phase: EditPhase) =>
    ed.send({ type: 'setPageBackground', color: toHex({ ...rgb, a: Math.round(alpha * 255) }), phase });
  return (
    <Section title="Страница">
      <div class="fe-paint">
        <button
          class="fe-swatch"
          aria-label="Фон страницы"
          aria-haspopup="dialog"
          aria-expanded={!!picker()}
          style={{ '--swatch': cssColor(c().rgb, c().alpha) }}
          onClick={(e) => setPicker(picker() ? null : e.currentTarget)}
        />
        <CommitInput
          name="HEX фона страницы"
          class="fe-row__grow"
          text={toHex6(c().rgb)}
          maxLength={10}
          capture={() => NO_SELECTION}
          onCommit={(text) => {
            const typed = parseHex(text);
            if (!typed || toHex6(typed) === toHex6(c().rgb)) return;
            set(typed, c().alpha, 'commit');
          }}
        />
        <Show when={bg()}>
          <button
            class="fe-tool fe-tool--small fe-tool--muted"
            aria-label="Фон как в теме"
            data-tip="Фон как в теме"
            onClick={() => ed.send({ type: 'setPageBackground', color: undefined, phase: 'commit' })}
          >
            <Icon name="close" />
          </button>
        </Show>
      </div>
      <Show when={picker()}>
        {(a) => (
          <ColorPicker
            title="Фон страницы"
            color={c().rgb}
            alpha={c().alpha}
            anchor={a()}
            begin={() => NO_SELECTION}
            onChange={(rgb, alpha, phase) => set(rgb, alpha, phase)}
            onClose={() => setPicker(null)}
          />
        )}
      </Show>
    </Section>
  );
}
