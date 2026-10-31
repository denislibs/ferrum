// Секция «Текст» панели свойств (спека подсистемы 3, раздел 7): шрифт (выбор с
// поиском), начертание, размер, интерлиньяж, трекинг, интервал и отступ абзаца,
// выравнивание по горизонтали и вертикали, режим размера, регистр,
// подчёркивание и зачёркивание, списки. Сводку («Смешанные» — у разных значений)
// готовит ядро: в правке текста — по выделенному диапазону, иначе — по всем
// символам выделенных текстов; правка — так же (`setTextStyle`, `setTextProps`).
// Поля чисел — превью до отпускания, одна правка — один шаг.

import { createEffect, createSignal, For, Show } from 'solid-js';
import type { EditPhase, SpacingDto, TextPropsPatchDto, TextStylePatchDto } from '../wasm/ferrum_wasm';
import { Section } from './chrome';
import { useEditor, type Editor } from './context';
import { FontPicker } from './font-picker';
import { Caret, Icon, type IconName } from './icons';
import { captureEdit, NumberField, type EditTarget } from './number-field';
import {
  fieldValue,
  formatSpacing,
  PARAGRAPH_INDENT_RANGE,
  PARAGRAPH_SPACING_RANGE,
  parseSpacing,
  SIZE_RANGE,
  styleFor,
} from './text-panel';

const MIXED = 'Смешанные';

/** Адресат правки: в правке текста — диапазон ядра (без id), иначе — выделенные на начало. */
function target(ed: Editor, t?: EditTarget): { ids: string[] | undefined } {
  if (ed.store.state.text.editing) return { ids: undefined };
  return { ids: (t ?? captureEdit(ed).target).ids };
}

function sendStyle(ed: Editor, patch: TextStylePatchDto, phase: EditPhase, t?: EditTarget) {
  ed.send({ type: 'setTextStyle', patch, phase, ...target(ed, t) });
}

function sendProps(ed: Editor, patch: TextPropsPatchDto, phase: EditPhase, t?: EditTarget) {
  ed.send({ type: 'setTextProps', patch, phase, ...target(ed, t) });
}

/** Поле интерлиньяжа или трекинга: «Авто», «150%», «20» — Enter или уход применяют,
 * Esc возвращает показанное. */
function SpacingField(props: {
  label: string;
  value: SpacingDto;
  mixed: boolean;
  auto: boolean;
  onCommit: (v: SpacingDto) => void;
}) {
  let input!: HTMLInputElement;
  const shown = () => (props.mixed ? '' : formatSpacing(props.value));
  createEffect(() => {
    if (document.activeElement !== input) input.value = shown();
  });
  const commit = () => {
    const v = parseSpacing(input.value, props.auto);
    if (v && (props.mixed || v.unit !== props.value.unit || v.value !== props.value.value)) props.onCommit(v);
    input.value = shown();
  };
  return (
    <label class="fe-field fe-num">
      <span class="fe-field__label">{props.label}</span>
      <input
        ref={input}
        class="fe-input t-num"
        aria-label={props.label === 'LH' ? 'Интерлиньяж' : 'Трекинг'}
        placeholder={props.mixed ? MIXED : ''}
        onFocus={(e) => e.currentTarget.select()}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            input.value = shown();
            input.blur();
          }
        }}
      />
    </label>
  );
}

/** Ряд переключателей с одним выбором (выравнивание, режим размера). */
function Choice<T extends string>(props: {
  label: string;
  value: T;
  mixed: boolean;
  items: { value: T; name: string; icon: IconName }[];
  onPick: (v: T) => void;
}) {
  return (
    <div class="fe-segmented fe-segmented--icons" role="radiogroup" aria-label={props.label}>
      <For each={props.items}>
        {(it) => (
          <button
            role="radio"
            class="fe-segmented__item"
            aria-checked={!props.mixed && props.value === it.value}
            aria-label={it.name}
            data-tip={it.name}
            onClick={() => props.onPick(it.value)}
          >
            <Icon name={it.icon} />
          </button>
        )}
      </For>
    </div>
  );
}

export function TextSection() {
  const ed = useEditor();
  const s = () => ed.store.state.textStyle!;
  const [picking, setPicking] = createSignal(false);
  const styles = () => ed.store.state.fontCatalog.find((c) => c.family === s().family.value)?.styles ?? [];
  const pickFamily = (family: string) => {
    setPicking(false);
    sendStyle(ed, { family, style: styleFor(ed.store.state.fontCatalog, family, s().style.value) }, 'commit');
  };
  const toggle = (key: 'decoration' | 'list', on: string, off: string) => {
    const cur = s()[key];
    const next = !cur.mixed && cur.value === on ? off : on;
    sendStyle(ed, { [key]: next } as TextStylePatchDto, 'commit');
  };
  const pressed = (key: 'decoration' | 'list', v: string) => !s()[key].mixed && s()[key].value === v;
  return (
    <Section title="Текст">
      <button
        class="fe-select fe-text__family"
        aria-haspopup="listbox"
        aria-expanded={picking()}
        aria-label="Шрифт"
        onClick={() => setPicking((p) => !p)}
      >
        <span class="fe-ellipsis" classList={{ 't-muted': s().family.mixed }}>
          {s().family.mixed ? MIXED : s().family.value}
        </span>
        <Caret />
      </button>
      <Show when={picking()}>
        <FontPicker current={s().family.value} onPick={pickFamily} onClose={() => setPicking(false)} />
      </Show>
      <div class="fe-grid2">
        <select
          class="fe-select"
          aria-label="Начертание"
          value={s().style.mixed ? '' : s().style.value}
          onChange={(e) =>
            !s().family.mixed && sendStyle(ed, { family: s().family.value, style: e.currentTarget.value }, 'commit')
          }
        >
          <Show when={s().style.mixed}>
            <option value="" disabled>
              {MIXED}
            </option>
          </Show>
          <For each={styles()}>{(st) => <option value={st}>{st}</option>}</For>
        </select>
        <NumberField
          label="Aa"
          name="Размер шрифта"
          value={s().size.value}
          mixed={s().size.mixed}
          min={SIZE_RANGE[0]}
          max={SIZE_RANGE[1]}
          onEdit={(input, phase, t) => sendStyle(ed, { size: fieldValue(input, s().size.value, ...SIZE_RANGE) }, phase, t)}
        />
        <SpacingField
          label="LH"
          value={s().lineHeight.value}
          mixed={s().lineHeight.mixed}
          auto
          onCommit={(v) => sendStyle(ed, { lineHeight: v }, 'commit')}
        />
        <SpacingField
          label="LS"
          value={s().letterSpacing.value}
          mixed={s().letterSpacing.mixed}
          auto={false}
          onCommit={(v) => sendStyle(ed, { letterSpacing: v }, 'commit')}
        />
        <NumberField
          label="¶"
          name="Интервал абзацев"
          value={s().paragraphSpacing.value}
          mixed={s().paragraphSpacing.mixed}
          min={PARAGRAPH_SPACING_RANGE[0]}
          max={PARAGRAPH_SPACING_RANGE[1]}
          onEdit={(input, phase, t) =>
            sendProps(ed, { paragraphSpacing: fieldValue(input, s().paragraphSpacing.value, ...PARAGRAPH_SPACING_RANGE) }, phase, t)
          }
        />
        <NumberField
          label="⇥"
          name="Отступ абзаца"
          value={s().paragraphIndent.value}
          mixed={s().paragraphIndent.mixed}
          min={PARAGRAPH_INDENT_RANGE[0]}
          max={PARAGRAPH_INDENT_RANGE[1]}
          onEdit={(input, phase, t) =>
            sendProps(ed, { paragraphIndent: fieldValue(input, s().paragraphIndent.value, ...PARAGRAPH_INDENT_RANGE) }, phase, t)
          }
        />
      </div>
      <div class="fe-row fe-text__row">
        <Choice
          label="Выравнивание по горизонтали"
          value={s().alignH.value}
          mixed={s().alignH.mixed}
          items={[
            { value: 'left', name: 'По левому краю', icon: 'alignLeft' },
            { value: 'center', name: 'По центру', icon: 'alignCenter' },
            { value: 'right', name: 'По правому краю', icon: 'alignRight' },
            { value: 'justify', name: 'По ширине', icon: 'alignJustify' },
          ]}
          onPick={(v) => sendProps(ed, { alignH: v as TextPropsPatchDto['alignH'] }, 'commit')}
        />
        <Choice
          label="Выравнивание по вертикали"
          value={s().alignV.value}
          mixed={s().alignV.mixed}
          items={[
            { value: 'top', name: 'По верху', icon: 'alignTop' },
            { value: 'middle', name: 'По середине', icon: 'alignMiddle' },
            { value: 'bottom', name: 'По низу', icon: 'alignBottom' },
          ]}
          onPick={(v) => sendProps(ed, { alignV: v as TextPropsPatchDto['alignV'] }, 'commit')}
        />
      </div>
      <div class="fe-row fe-text__row">
        <Choice
          label="Размер рамки"
          value={s().sizing.value}
          mixed={s().sizing.mixed}
          items={[
            { value: 'autoWidth', name: 'Авто по ширине', icon: 'autoWidth' },
            { value: 'autoHeight', name: 'Авто по высоте', icon: 'autoHeight' },
            { value: 'fixed', name: 'Фиксированный размер', icon: 'rect' },
          ]}
          onPick={(v) => sendProps(ed, { sizing: v as TextPropsPatchDto['sizing'] }, 'commit')}
        />
        <select
          class="fe-select fe-row__grow"
          aria-label="Регистр"
          value={s().case.mixed ? '' : s().case.value}
          onChange={(e) => sendStyle(ed, { case: e.currentTarget.value as TextStylePatchDto['case'] }, 'commit')}
        >
          <Show when={s().case.mixed}>
            <option value="" disabled>
              {MIXED}
            </option>
          </Show>
          <option value="none">Как есть</option>
          <option value="upper">ВЕРХНИЙ</option>
          <option value="lower">нижний</option>
          <option value="title">Каждое Слово</option>
          <option value="smallCaps">Капитель</option>
        </select>
      </div>
      <div class="fe-row fe-text__row">
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('decoration', 'underline')}
          aria-label="Подчёркнутый"
          data-tip="Подчёркнутый"
          onClick={() => toggle('decoration', 'underline', 'none')}
        >
          <Icon name="underline" />
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('decoration', 'strike')}
          aria-label="Зачёркнутый"
          data-tip="Зачёркнутый"
          onClick={() => toggle('decoration', 'strike', 'none')}
        >
          <Icon name="strike" />
        </button>
        <span class="fe-toolbar__sep" role="separator" />
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('list', 'bullet')}
          aria-label="Маркированный список"
          data-tip="Маркированный список"
          onClick={() => toggle('list', 'bullet', 'none')}
        >
          <Icon name="list" />
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-pressed={pressed('list', 'ordered')}
          aria-label="Нумерованный список"
          data-tip="Нумерованный список"
          onClick={() => toggle('list', 'ordered', 'none')}
        >
          <Icon name="listOrdered" />
        </button>
      </div>
    </Section>
  );
}
