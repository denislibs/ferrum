// Панель свойств («Дизайн»): сводку выделения готовит worker (значения и
// «Смешанные»), панель только показывает её и шлёт правки командами. Во время
// жеста на холсте значения «Положения» одного узла — из живого состояния (общая
// память, фаза measure), а не из событий. Секции по порядку дизайн-системы:
// Фрейм / Положение → Вид → Заливка → Обводка → Экспорт. Без выделения —
// страница (фон) и экспорт всей страницы. У инструмента «Масштабирование» (K)
// сверху — секция «Масштаб»: поле в процентах и точка рамки 3×3, как в Figma.

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { Edit, EditPhase, ExportFormat, Field, Num } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor } from './context';
import { Caret, Icon } from './icons';
import { Menu, type MenuItem } from './menu';
import { NumberField, type EditTarget } from './number-field';
import type { FieldInput } from './number';
import { MissingFontsSection } from './missing-fonts';
import { FillsSection, PageSection, StrokesSection } from './paints';
import { TextSection } from './text-section';

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
    <div class="fe-props" id="panel-design" role="tabpanel" aria-labelledby="tab-design" tabindex="-1">
      <Show
        when={count() > 0}
        fallback={
          <Show
            when={ed.store.state.text.editing && ed.store.state.textStyle}
            fallback={
              <>
                <MissingFontsSection />
                <PageSection theme={props.theme} />
              </>
            }
          >
            {/* Новый текст (инструмент T) до первого символа: стиль — заранее. */}
            <TextSection />
          </Show>
        }
      >
        <Show when={ed.store.state.tool === 'scale'}>
          <ScaleSection />
        </Show>
        <PositionSection edit={edit} val={val} />
        <AppearanceSection edit={edit} />
        <Show when={ed.store.state.textStyle}>
          <TextSection />
        </Show>
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

/** Точки рамки для масштаба (сетка 3×3 по строкам), подписи — для чтения с экрана. */
const ANCHORS: { at: [number, number]; name: string }[] = [
  { at: [0, 0], name: 'Слева сверху' },
  { at: [0.5, 0], name: 'Сверху по центру' },
  { at: [1, 0], name: 'Справа сверху' },
  { at: [0, 0.5], name: 'Слева по центру' },
  { at: [0.5, 0.5], name: 'По центру' },
  { at: [1, 0.5], name: 'Справа по центру' },
  { at: [0, 1], name: 'Слева снизу' },
  { at: [0.5, 1], name: 'Снизу по центру' },
  { at: [1, 1], name: 'Справа снизу' },
];

/** Масштаб в процентах: те же пределы, что у ядра (1 %…10 000 %). */
export const SCALE_PERCENT_RANGE = [1, 10_000] as const;

/** Коэффициент масштаба из ввода поля «Масштаб» (100 % — без изменений). */
export function scaleFactor(input: FieldInput): number {
  const percent = input.kind === 'set' ? input.value : 100 + input.delta;
  return Math.min(SCALE_PERCENT_RANGE[1], Math.max(SCALE_PERCENT_RANGE[0], percent)) / 100;
}

function ScaleSection() {
  const ed = useEditor();
  // Точка — левый верхний угол, как у полей W и H (Figma).
  const [anchor, setAnchor] = createSignal(0);
  let grid!: HTMLDivElement;
  const onKey = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    const at = anchor();
    const [col, row] = [at % 3, Math.floor(at / 3)];
    let next: number;
    switch (e.key) {
      case 'ArrowRight':
        next = row * 3 + ((col + 1) % 3);
        break;
      case 'ArrowLeft':
        next = row * 3 + ((col + 2) % 3);
        break;
      case 'ArrowDown':
        next = ((row + 1) % 3) * 3 + col;
        break;
      case 'ArrowUp':
        next = ((row + 2) % 3) * 3 + col;
        break;
      default:
        return;
    }
    e.preventDefault();
    setAnchor(next);
    grid.querySelectorAll<HTMLElement>('[role="radio"]')[next]?.focus();
  };
  return (
    <Section title="Масштаб">
      <div class="fe-scale">
        <div ref={grid} class="fe-anchor" role="radiogroup" aria-label="Точка масштаба" onKeyDown={onKey}>
          <For each={ANCHORS}>
            {(a, i) => (
              <button
                role="radio"
                aria-checked={anchor() === i()}
                aria-label={a.name}
                tabindex={anchor() === i() ? 0 : -1}
                class="fe-anchor__dot"
                onClick={() => setAnchor(i())}
              />
            )}
          </For>
        </div>
        <NumberField
          label={<Icon name="scale" />}
          name="Масштаб"
          value={100}
          suffix="%"
          decimals={2}
          min={SCALE_PERCENT_RANGE[0]}
          max={SCALE_PERCENT_RANGE[1]}
          percentOf={100}
          onEdit={(input, phase, target) =>
            sendEdit(ed.send, { type: 'scale', factor: scaleFactor(input), anchor: ANCHORS[anchor()].at }, phase, target)
          }
          class="fe-row__grow"
        />
      </div>
    </Section>
  );
}

const SCALES = [1, 2, 3] as const;

/** Форматы экспорта, как в разделе «Export» Figma (без JPG: его нет в ядре). */
const FORMATS: { id: ExportFormat; label: string }[] = [
  { id: 'png', label: 'PNG' },
  { id: 'svg', label: 'SVG' },
  { id: 'pdf', label: 'PDF' },
];

function ExportSection() {
  const ed = useEditor();
  const [scale, setScale] = createSignal<number>(1);
  const [format, setFormat] = createSignal<ExportFormat>('png');
  // Настройки SVG, как в Figma: «Текст кривыми» включён, «Атрибут id» — нет.
  const [outlineText, setOutlineText] = createSignal(true);
  const [svgIds, setSvgIds] = createSignal(false);
  let group!: HTMLDivElement;
  // Группа переключателей: одна точка Tab (выбранный), стрелки и Home/End выбирают
  // соседний и переводят на него фокус.
  const onKey = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    const at = SCALES.indexOf(scale() as (typeof SCALES)[number]);
    const n = SCALES.length;
    let next: number;
    switch (e.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        next = (at + 1) % n;
        break;
      case 'ArrowLeft':
      case 'ArrowUp':
        next = (at - 1 + n) % n;
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = n - 1;
        break;
      default:
        return;
    }
    e.preventDefault();
    setScale(SCALES[next]);
    group.querySelectorAll<HTMLElement>('[role="radio"]')[next]?.focus();
  };
  const target = () => {
    const s = ed.store.state.summary;
    if (!s || s.count === 0) return 'страницу';
    return s.count === 1 ? `«${s.name ?? ''}»` : 'выделение';
  };
  // Масштаб — у PNG; SVG и PDF векторные, в единицах документа (1 px = 1 pt).
  const raster = () => format() === 'png';
  const run = () =>
    ed.send({
      type: 'export',
      scale: raster() ? scale() : 1,
      format: format(),
      outlineText: format() === 'svg' ? outlineText() : undefined,
      svgIds: format() === 'svg' ? svgIds() : undefined,
    });
  return (
    <Section title="Экспорт">
      <div class="fe-export">
        <Show when={raster()}>
          <div ref={group} class="fe-segmented" role="radiogroup" aria-label="Масштаб экспорта" onKeyDown={onKey}>
            <For each={SCALES}>
              {(k) => (
                <button
                  role="radio"
                  aria-checked={scale() === k}
                  tabindex={scale() === k ? 0 : -1}
                  class="fe-segmented__item t-num"
                  onClick={() => setScale(k)}
                >
                  {k}×
                </button>
              )}
            </For>
          </div>
        </Show>
        <select
          class="fe-format t-num"
          aria-label="Формат экспорта"
          value={format()}
          onChange={(e) => setFormat(e.currentTarget.value as ExportFormat)}
        >
          <For each={FORMATS}>{(f) => <option value={f.id}>{f.label}</option>}</For>
        </select>
      </div>
      <Show when={format() === 'svg'}>
        <label class="fe-check">
          <input type="checkbox" checked={outlineText()} onChange={(e) => setOutlineText(e.currentTarget.checked)} />
          <span>Текст кривыми</span>
        </label>
        <label class="fe-check">
          <input type="checkbox" checked={svgIds()} onChange={(e) => setSvgIds(e.currentTarget.checked)} />
          <span>Атрибут id</span>
        </label>
      </Show>
      <button class="fe-btn fe-btn--soft fe-btn--wide" data-export-run onClick={run}>
        <Icon name="download" />
        <span class="fe-ellipsis">Экспортировать {target()}</span>
      </button>
    </Section>
  );
}
