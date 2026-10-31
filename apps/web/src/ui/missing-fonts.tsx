// «Отсутствующие шрифты (N)» (спека подсистемы 3, раздел 4): шрифты текущей
// страницы, которых нет ни во вшитых, ни в системных, с заменой одним действием
// (одна правка — один шаг отмены). Список и каталог семейств присылает ядро;
// панель показывает их и шлёт `replaceMissingFont`. Как в Figma — на панели
// «Дизайн» без выделения. «Показать системные шрифты» — в выборе шрифта секции
// «Текст» (этап 2).

import { createMemo, createSignal, For, Show } from 'solid-js';
import type { FontChoice, MissingFontDto } from '../wasm/ferrum_wasm';
import { defaultStyle } from '../core/fonts';
import { Section } from './chrome';
import { useEditor } from './context';

function MissingRow(props: { font: MissingFontDto; catalog: FontChoice[] }) {
  const ed = useEditor();
  const [family, setFamily] = createSignal('Inter');
  const choice = () => props.catalog.find((c) => c.family === family());
  const [style, setStyle] = createSignal<string | null>(null);
  const effectiveStyle = () => style() ?? defaultStyle(choice(), props.font.style);
  const bundled = () => props.catalog.filter((c) => c.source === 'bundled');
  const system = () => props.catalog.filter((c) => c.source === 'system');
  const name = () => `${props.font.family} ${props.font.style}`;
  return (
    <div class="fe-missing" role="group" aria-label={name()}>
      <div class="fe-missing__name">
        <span class="fe-ellipsis">{name()}</span>
        <span class="t-muted t-num">{props.font.nodes}</span>
      </div>
      <div class="fe-missing__pick">
        <select
          class="fe-select fe-missing__family"
          aria-label={`Замена для ${name()}: семейство`}
          value={family()}
          onChange={(e) => {
            setFamily(e.currentTarget.value);
            setStyle(null);
          }}
        >
          <optgroup label="Вшитые">
            <For each={bundled()}>{(c) => <option value={c.family}>{c.family}</option>}</For>
          </optgroup>
          <Show when={system().length > 0}>
            <optgroup label="Системные">
              <For each={system()}>{(c) => <option value={c.family}>{c.family}</option>}</For>
            </optgroup>
          </Show>
        </select>
        <select
          class="fe-select fe-missing__style"
          aria-label={`Замена для ${name()}: начертание`}
          value={effectiveStyle()}
          onChange={(e) => setStyle(e.currentTarget.value)}
        >
          <For each={choice()?.styles ?? []}>{(s) => <option value={s}>{s}</option>}</For>
        </select>
      </div>
      <button
        class="fe-btn fe-btn--soft fe-btn--wide"
        disabled={!choice() || !effectiveStyle()}
        onClick={() =>
          ed.send({
            type: 'replaceMissingFont',
            from: { family: props.font.family, style: props.font.style },
            to: { family: family(), style: effectiveStyle() },
          })
        }
      >
        Заменить
      </button>
    </div>
  );
}

export function MissingFontsSection() {
  const ed = useEditor();
  const list = () => ed.store.state.missingFonts;
  const catalog = createMemo(() => [...ed.store.state.fontCatalog].sort((a, b) => a.family.localeCompare(b.family)));
  return (
    <Show when={list().length > 0}>
      <Section title={`Отсутствующие шрифты (${list().length})`}>
        <For each={list()}>{(f) => <MissingRow font={f} catalog={catalog()} />}</For>
      </Section>
    </Show>
  );
}
