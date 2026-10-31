// Выбор шрифта секции «Текст» (спека подсистемы 3, раздел 7): поиск, группы
// «В документе» / «Вшитые» / «Системные», виртуальный список (тысячи системных
// семейств — в DOM только видимые строки), название — своим шрифтом. Вшитые для
// предпросмотра грузит браузер (`FontFace` с файла сборки — его же HTTP-кэш, что у
// worker'а ядра), системные Chrome и десктопа — по имени. «Показать системные
// шрифты» — здесь же.

import { createMemo, createSignal, For, onMount, Show } from 'solid-js';
import { manifestFamilies } from '../core/fonts';
import { useEditor } from './context';
import { fontRows, rowWindow, type FontRow } from './text-panel';

const ROW = 28;
const LIST_HEIGHT = 280;
/** Имя `FontFace` предпросмотра вшитого семейства (своё — не путать с шрифтами интерфейса). */
const PREVIEW = 'ferrum-preview-';

/** Файл обычного начертания каждого вшитого семейства (манифест сборки). */
let manifest: Promise<Map<string, string>> | null = null;
const previewed = new Set<string>();

function bundledFiles(): Promise<Map<string, string>> {
  manifest ??= fetch(new URL('/fonts/manifest.json', location.origin).href)
    .then((r) => (r.ok ? r.json() : { families: [] }))
    .then((json: unknown) => {
      const map = new Map<string, string>();
      for (const f of manifestFamilies(json)) {
        const face = f.faces.find((x) => /regular/i.test(x.style)) ?? f.faces[0];
        if (face) map.set(f.family, face.file);
      }
      return map;
    })
    .catch(() => new Map());
  return manifest;
}

/** CSS-семейство предпросмотра названия; вшитое — догружается при первом показе. */
function previewFamily(family: string, source: 'bundled' | 'system'): string {
  const quoted = `"${family.replace(/["\\]/g, '')}"`;
  if (source !== 'bundled') return `${quoted}, var(--font-sans)`;
  if (!previewed.has(family)) {
    previewed.add(family);
    void bundledFiles().then((files) => {
      const file = files.get(family);
      if (!file) return;
      const face = new FontFace(PREVIEW + family, `url(/fonts/${file})`);
      face.load().then(
        (f) => document.fonts.add(f),
        () => {},
      );
    });
  }
  return `"${PREVIEW}${family.replace(/["\\]/g, '')}", var(--font-sans)`;
}

export function FontPicker(props: { current: string; onPick: (family: string) => void; onClose: () => void }) {
  const ed = useEditor();
  const [query, setQuery] = createSignal('');
  const [top, setTop] = createSignal(0);
  const rows = createMemo(() => fontRows(ed.store.state.fontCatalog, ed.store.state.pageFonts, query()));
  const visible = createMemo(() => {
    const w = rowWindow(top(), LIST_HEIGHT, ROW, rows().length);
    return rows()
      .slice(w.start, w.end)
      .map((r, i) => ({ row: r, index: w.start + i }));
  });
  const sourceOf = (family: string): 'bundled' | 'system' =>
    ed.store.state.fontCatalog.find((c) => c.family === family)?.source ?? 'system';
  const sys = () => ed.store.state.systemFonts;
  let list!: HTMLDivElement;
  let search!: HTMLInputElement;
  onMount(() => search.focus());
  // Прокрутка читается в фазе measure (защита layout тестового режима).
  const onScroll = ed.core.costed(() => ed.scheduler.measure(() => setTop(list.scrollTop)));
  const first = () => rows().find((r): r is FontRow & { kind: 'font' } => r.kind === 'font');
  return (
    <div class="fe-fonts" role="group" aria-label="Выбор шрифта">
      <input
        ref={search}
        class="fe-input fe-fonts__search"
        type="search"
        aria-label="Поиск шрифта"
        placeholder="Поиск шрифта"
        value={query()}
        onInput={(e) => {
          setQuery(e.currentTarget.value);
          setTop(0);
          list.scrollTop = 0;
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            props.onClose();
          } else if (e.key === 'Enter') {
            e.preventDefault();
            const f = first();
            if (f) props.onPick(f.family);
          }
        }}
      />
      <div ref={list} class="fe-fonts__list" role="listbox" aria-label="Шрифты" style={{ height: `${LIST_HEIGHT}px` }} onScroll={onScroll}>
        <div style={{ height: `${rows().length * ROW}px`, position: 'relative' }}>
          <For each={visible()}>
            {(v) =>
              v.row.kind === 'heading' ? (
                <div class="fe-fonts__heading t-muted" role="presentation" style={{ top: `${v.index * ROW}px` }}>
                  {v.row.label}
                </div>
              ) : (
                <div
                  class="fe-fonts__row"
                  role="option"
                  aria-selected={v.row.family === props.current}
                  style={{
                    top: `${v.index * ROW}px`,
                    'font-family': previewFamily(v.row.family, v.row.source === 'document' ? sourceOf(v.row.family) : v.row.source),
                  }}
                  onClick={() => v.row.kind === 'font' && props.onPick(v.row.family)}
                >
                  {v.row.family}
                </div>
              )
            }
          </For>
        </div>
      </div>
      <Show when={rows().length === 0}>
        <p class="t-muted fe-fonts__empty">Ничего не найдено</p>
      </Show>
      <Show when={sys().supported && !sys().shown}>
        <button class="fe-btn fe-btn--soft fe-btn--wide" onClick={() => ed.showSystemFonts()}>
          Показать системные шрифты
        </button>
      </Show>
    </div>
  );
}
