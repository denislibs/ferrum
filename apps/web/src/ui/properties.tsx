// Панель свойств («Дизайн»): сводку выделения готовит worker (значения и
// «Смешанные»), панель только показывает её и шлёт правки командами. Во время
// жеста на холсте значения «Положения» одного узла — из живого состояния (общая
// память, фаза measure), а не из событий. Секции по порядку дизайн-системы:
// Фрейм / Положение → Вид → Заливка → Обводка → Экспорт. Без выделения —
// страница (фон) и экспорт всей страницы.

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { Edit, EditPhase, Field, Num } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem } from './menu';
import { NumberField, type EditTarget } from './number-field';
import type { FieldInput } from './number';
import { FillsSection, PageSection, StrokesSection } from './paints';

/** Правка; `target` — кого правят с начала жеста (без него — текущее выделение). */
export function sendEdit(
  send: (c: {
    type: 'editSelection';
    edit: Edit;
    phase: EditPhase;
    ids: string[] | undefined;
    selectionVersion: number | undefined;
  }) => void,
  edit: Edit,
  phase: EditPhase,
  target?: EditTarget,
) {
  send({ type: 'editSelection', edit, phase, ids: target?.ids, selectionVersion: target?.selectionVersion });
}

/** Правка числового поля командой (значение или прибавка к каждому). */
function numberEdit(field: Field, input: FieldInput): Edit {
  return input.kind === 'set'
    ? { type: 'number', field, value: input.value, relative: false }
    : { type: 'number', field, value: input.delta, relative: true };
}

export function PropertiesPanel(props: { theme: 'dark' | 'light' }) {
  const ed = useEditor();
  const s = () => ed.store.state.summary;
  const count = () => s()?.count ?? 0;
  const edit = (field: Field) => (input: FieldInput, phase: EditPhase, target: EditTarget) =>
    sendEdit(ed.send, numberEdit(field, input), phase, target);
  // Один узел во время жеста холста — живые значения из общей памяти.
  const live = () => ed.store.state.live;
  const liveOne = () => live().busy && live().count === 1 && count() === 1;
  const val = (n: Num | undefined, liveValue: number): { value: number; mixed: boolean } =>
    liveOne() ? { value: liveValue, mixed: false } : { value: n?.value ?? 0, mixed: n?.mixed ?? false };

  return (
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design">
      <Show when={count() > 0} fallback={<PageSection theme={props.theme} />}>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <FillsSection />
        <StrokesSection />
      </Show>
      <ExportSection />
    </div>
  );
}

function PositionSection(props: {
  edit: (f: Field) => (i: FieldInput, p: EditPhase, t: EditTarget) => void;
  val: (n: Num | undefined, live: number) => { value: number; mixed: boolean };
}) {
  const ed = useEditor();
  const s = () => ed.store.state.summary!;
  const live = () => ed.store.state.live;
  const frame = () => s().kind === 'frame';
  const [presets, setPresets] = createSignal<HTMLElement | null>(null);
  const presetItems = (): MenuItem[] => {
    const out: MenuItem[] = [];
    let group = '';
    for (const p of ed.store.state.presets) {
      if (p.group !== group) {
        group = p.group;
        if (out.length) out.push({ separator: true });
        out.push({ heading: group });
      }
      out.push({
        label: p.name,
        hint: `${p.width} × ${p.height}`,
        checked: s().w.value === p.width && s().h?.value === p.height,
        onSelect: () => sendEdit(ed.send, { type: 'size', w: p.width, h: p.height }, 'commit'),
      });
    }
    return out;
  };
  const presetName = () =>
    ed.store.state.presets.find((p) => p.width === s().w.value && p.height === s().h?.value && !s().w.mixed)?.name ?? 'Свой размер';
  const x = () => props.val(s().x, live().x);
  const y = () => props.val(s().y, live().y);
  const w = () => props.val(s().w, live().w);
  const h = () => props.val(s().h, live().h);
  const r = () => props.val(s().rotation, live().rotation);
  return (
    <Section title={frame() ? 'Фрейм' : 'Положение'}>
      <Show when={frame() && s().count === 1}>
        <button
          class="fe-select"
          aria-haspopup="menu"
          aria-expanded={!!presets()}
          aria-label="Пресет размера"
          onClick={(e) => setPresets(e.currentTarget)}
        >
          <span>{presetName()}</span>
          <Caret />
        </button>
        <Show when={presets()}>
          {(a) => <Menu label="Пресеты размера" items={presetItems()} anchor={a()} onClose={() => setPresets(null)} />}
        </Show>
      </Show>
      <div class="fe-grid2">
        <NumberField label="X" name="X" value={x().value} mixed={x().mixed} onEdit={props.edit('x')} />
        <NumberField label="Y" name="Y" value={y().value} mixed={y().mixed} onEdit={props.edit('y')} />
        <NumberField label="W" name="Ширина" value={w().value} mixed={w().mixed} min={0.01} onEdit={props.edit('w')} />
        <NumberField
          label="H"
          name="Высота"
          value={h().value}
          mixed={h().mixed}
          min={0.01}
          disabled={!s().h}
          onEdit={props.edit('h')}
        />
      </div>
      <div class="fe-row">
        <NumberField
          label={<Icon name="rotate" />}
          name="Поворот"
          value={r().value}
          mixed={r().mixed}
          suffix="°"
          onEdit={props.edit('rotation')}
          class="fe-row__grow"
        />
        <button
          class="fe-tool fe-tool--small"
          aria-label="Отразить по горизонтали"
          data-tip="Отразить по горизонтали"
          data-command="object.flip_horizontal"
          onClick={() => ed.run('object.flip_horizontal')}
        >
          <Icon name="flipH" />
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-label="Отразить по вертикали"
          data-tip="Отразить по вертикали"
          data-command="object.flip_vertical"
          onClick={() => ed.run('object.flip_vertical')}
        >
          <Icon name="flipV" />
        </button>
      </div>
    </Section>
  );
}

function AppearanceSection(props: { edit: (f: Field) => (i: FieldInput, p: EditPhase, t: EditTarget) => void }) {
  const ed = useEditor();
  const s = () => ed.store.state.summary!;
  return (
    <Section title="Вид">
      <div class="fe-grid2">
        <NumberField
          label={<Icon name="opacity" />}
          name="Прозрачность"
          value={s().opacity.value}
          mixed={s().opacity.mixed}
          scale={100}
          suffix="%"
          decimals={0}
          min={0}
          max={100}
          percentOf={100}
          onEdit={props.edit('opacity')}
        />
        <Show when={s().cornerRadius}>
          {(radius) => (
            <NumberField
              label={<Icon name="radius" />}
              name="Радиус скругления"
              value={radius().value}
              mixed={radius().mixed}
              min={0}
              onEdit={props.edit('cornerRadius')}
            />
          )}
        </Show>
      </div>
      <Show when={s().clipContent}>
        {(clip) => (
          <label class="fe-check">
            <input
              type="checkbox"
              checked={clip().value && !clip().mixed}
              ref={(el) => createEffect(() => (el.indeterminate = clip().mixed))}
              onChange={(e) => sendEdit(ed.send, { type: 'clipContent', value: e.currentTarget.checked }, 'commit')}
            />
            <span>Обрезать содержимое</span>
          </label>
        )}
      </Show>
    </Section>
  );
}

const SCALES = [1, 2, 3] as const;

function ExportSection() {
  const ed = useEditor();
  const [scale, setScale] = createSignal<number>(1);
  const target = () => {
    const s = ed.store.state.summary;
    if (!s || s.count === 0) return 'страницу';
    return s.count === 1 ? `«${s.name ?? ''}»` : 'выделение';
  };
  return (
    <Section title="Экспорт">
      <div class="fe-row">
        <div class="fe-segmented" role="radiogroup" aria-label="Масштаб экспорта">
          <For each={SCALES}>
            {(k) => (
              <button role="radio" aria-checked={scale() === k} class="fe-segmented__item t-num" onClick={() => setScale(k)}>
                {k}×
              </button>
            )}
          </For>
        </div>
        <span class="fe-format t-num" aria-label="Формат">
          PNG
        </span>
      </div>
      <button class="fe-btn fe-btn--secondary fe-btn--wide" onClick={() => ed.send({ type: 'export', scale: scale() })}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
      </button>
    </Section>
  );
}
