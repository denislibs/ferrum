// Выбор шрифта секции «Текст» (спека подсистемы 3, раздел 7): поиск, группы
// «В документе» / «Вшитые» / «Системные», виртуальный список (тысячи системных
// семейств — в DOM только видимые строки), название — своим шрифтом. Вшитые для
// предпросмотра грузит браузер (`FontFace` с файла сборки — его же HTTP-кэш, что у
// worker'а ядра), системные Chrome и десктопа — по имени. «Показать системные
// шрифты» — здесь же.

import { createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { manifestFamilies } from '../core/fonts';
import { useEditor } from './context';
import { fontRows, isImeKey, rowWindow, stepRow, type FontRow } from './text-panel';

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
  // Окно — два числа: строки в нём те же объекты при прокрутке, и `For` не пересоздаёт узлы.
  const win = createMemo(() => rowWindow(top(), LIST_HEIGHT, ROW, rows().length));
  const start = createMemo(() => win().start);
  const end = createMemo(() => win().end);
  const visible = createMemo(() => rows().slice(start(), end()));
  // Строка под стрелками (индекс в `rows`); -1 — нет.
  const [cursor, setCursor] = createSignal(-1);
  const sourceOf = (family: string): 'bundled' | 'system' =>
    ed.store.state.fontCatalog.find((c) => c.family === family)?.source ?? 'system';
  const sys = () => ed.store.state.systemFonts;
  let list!: HTMLDivElement;
  let search!: HTMLInputElement;
  let root!: HTMLDivElement;
  onMount(() => {
    search.focus();
    // Щелчок снаружи закрывает; кнопка шрифта сама переключает список.
    const outside = (e: PointerEvent) => {
      const t = e.target;
      if (t instanceof Element && !root.contains(t) && !t.closest('[aria-haspopup="listbox"]')) props.onClose();
    };
    document.addEventListener('pointerdown', outside, true);
    onCleanup(() => document.removeEventListener('pointerdown', outside, true));
  });
  const reveal = (index: number) => {
    const y = index * ROW;
    if (y < list.scrollTop) list.scrollTop = y;
    else if (y + ROW > list.scrollTop + LIST_HEIGHT) list.scrollTop = y + ROW - LIST_HEIGHT;
    setTop(list.scrollTop);
  };
  // Прокрутка читается в фазе measure (защита layout тестового режима).
  const onScroll = ed.core.costed(() => ed.scheduler.measure(() => setTop(list.scrollTop)));
  const pickAt = (index: number) => {
    const r: FontRow | undefined = rows()[index];
    if (r?.kind === 'font') props.onPick(r.family);
  };
  return (
    <div
      ref={root}
      class="fe-fonts"
      role="group"
      aria-label="Выбор шрифта"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          props.onClose();
        }
      }}
    >
      <input
        ref={search}
        class="fe-input fe-fonts__search"
        type="search"
        aria-label="Поиск шрифта"
        placeholder="Поиск шрифта"
        value={query()}
        aria-activedescendant={cursor() >= 0 ? `fe-font-${cursor()}` : undefined}
        onInput={(e) => {
          setCursor(-1);
          setQuery(e.currentTarget.value);
          setTop(0);
          list.scrollTop = 0;
        }}
        onKeyDown={(e) => {
          // Enter, подтверждающий вариант IME, — не выбор шрифта.
          if (isImeKey(e)) return;
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            const next = stepRow(rows(), cursor(), e.key === 'ArrowDown' ? 1 : -1);
            setCursor(next);
            if (next >= 0) reveal(next);
          } else if (e.key === 'Enter') {
            e.preventDefault();
            pickAt(cursor() >= 0 ? cursor() : stepRow(rows(), -1, 1));
          }
        }}
      />
      <div ref={list} class="fe-fonts__list" role="listbox" aria-label="Шрифты" style={{ height: `${LIST_HEIGHT}px` }} onScroll={onScroll}>
        <div style={{ height: `${rows().length * ROW}px`, position: 'relative' }}>
          <For each={visible()}>
            {(row, i) => {
              const index = () => start() + i();
              return row.kind === 'heading' ? (
                <div class="fe-fonts__heading t-muted" role="presentation" style={{ top: `${index() * ROW}px` }}>
                  {row.label}
                </div>
              ) : (
                <div
                  id={`fe-font-${index()}`}
                  class="fe-fonts__row"
                  classList={{ 'fe-fonts__row--active': cursor() === index() }}
                  role="option"
                  aria-selected={row.family === props.current}
                  style={{
                    top: `${index() * ROW}px`,
                    'font-family': previewFamily(row.family, row.source === 'document' ? sourceOf(row.family) : row.source),
                  }}
                  onClick={() => props.onPick(row.family)}
                >
                  {row.family}
                </div>
              );
            }}
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
