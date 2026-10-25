// Экран файлов (как «Recents» в Figma): разделы «Недавние», «Черновики», «Корзина»,
// поиск по имени, карточки сеткой или строки списком, меню файла, переименование,
// корзина. Worker ядра здесь не запускается (быстрый первый показ): всё о файлах —
// в worker'е ввода-вывода, основной поток получает только окно видимых записей
// (виртуальный список: DOM не растёт с числом файлов) и превью Blob'ами, адреса
// которых освобождаются, когда превью сменилось или экран закрыт.

import { batch, createEffect, createMemo, createSignal, For, Index, on, onCleanup, onMount, Show, untrack, type JSX } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { MAX_WINDOW } from '../core/file-list';
import { filesLayout, filesWindow, GRID_GAP } from './files-layout';
import { dismissSplash } from './boot';
import { ConfirmDialog } from './dialog';
import { downloadBytes, fileName, formatSize, relativeTime } from './files';
import { importFromDisk, type FileMeta, type FileStore, type Section } from './file-store';
import { Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
import { RenameInput } from './rename-input';
import type { Route } from './router';
import { loadSettings, resolveTheme, storage, updateSettings, type FilesView } from './settings';
import { createUiStore } from './store';
import { Toasts } from './toasts';

const SECTIONS: { id: Section; title: string; icon: IconName }[] = [
  { id: 'recent', title: 'Недавние', icon: 'clock' },
  { id: 'drafts', title: 'Черновики', icon: 'file' },
  { id: 'trash', title: 'Корзина', icon: 'trash' },
];

export interface FilesScreenProps {
  files: FileStore;
  section: Section;
  /** Сообщение (файл не найден, не открылся); `seq` — чтобы то же сообщение показалось снова. */
  notice: { text: string; seq: number } | null;
  navigate: (r: Route) => void;
}

type Confirm = { kind: 'delete'; file: FileMeta } | { kind: 'empty' };

export function FilesScreen(props: FilesScreenProps) {
  const ui = createUiStore();
  const saved = loadSettings(storage());
  // Тему до первого рисования ставит theme-init.js, после редактора она уже на
  // <html>; здесь — только «как в системе», которая следует за системой на лету.
  const media = matchMedia('(prefers-color-scheme: dark)');
  const applyTheme = () => (document.documentElement.dataset.theme = resolveTheme(loadSettings(storage()).theme, media.matches));
  media.addEventListener('change', applyTheme);
  onCleanup(() => media.removeEventListener('change', applyTheme));

  const [view, setView] = createSignal<FilesView>(saved.filesView);
  const [query, setQuery] = createSignal('');
  const [total, setTotal] = createSignal(0);
  const [loaded, setLoaded] = createSignal(false);
  const [rows, setRows] = createStore<{ start: number; list: FileMeta[] }>({ start: 0, list: [] });
  const [width, setWidth] = createSignal(0);
  const [height, setHeight] = createSignal(0);
  const [scrollTop, setScrollTop] = createSignal(0);
  const [focused, setFocused] = createSignal(0);
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [menu, setMenu] = createSignal<{ file: FileMeta; anchor: HTMLElement | { x: number; y: number } } | null>(null);
  const [confirm, setConfirm] = createSignal<Confirm | null>(null);
  const [now, setNow] = createSignal(Date.now());
  const [thumbs, setThumbs] = createStore<Record<string, string>>({});
  const thumbVersions = new Map<string, number>();
  let scroller!: HTMLDivElement;
  let list!: HTMLDivElement;
  let nav!: HTMLDivElement;
  /** Перевести фокус на запись с этим номером, когда она окажется в DOM. */
  let pendingFocus: number | null = null;
  let alive = true;

  const layout = createMemo(() => filesLayout(width(), view()));
  const title = () => SECTIONS.find((s) => s.id === props.section)!.title;

  // ——— Окно списка ———
  let asked = '';
  let requestSeq = 0;
  const request = (force = false) => {
    const w = filesWindow(scrollTop(), height(), layout());
    const key = `${props.section}|${query()}|${w.start}|${w.count}`;
    if (!force && key === asked) return;
    asked = key;
    const my = ++requestSeq;
    props.files.list(props.section, query(), w.start, w.count).then(
      (r) => {
        if (!alive || my !== requestSeq) return;
        batch(() => {
          setTotal(r.total);
          setRows('start', w.start);
          setRows('list', reconcile(r.rows, { key: 'id' }));
          setLoaded(true);
          if (focused() >= r.total) setFocused(Math.max(0, r.total - 1));
        });
        loadThumbs(r.rows);
      },
      (e: unknown) => {
        if (alive) ui.toast(`Не удалось прочитать список файлов: ${e instanceof Error ? e.message : String(e)}`, 'error');
      },
    );
  };
  createEffect(on([() => props.section, scrollTop, height, layout, query], () => request()));
  // Другой раздел: с начала списка, фокус — на первой записи.
  createEffect(
    on(
      () => props.section,
      () => {
        batch(() => {
          setFocused(0);
          setRenaming(null);
          setLoaded(false);
        });
        scroller.scrollTop = 0;
      },
      { defer: true },
    ),
  );
  createEffect(on(() => props.notice, (n) => n && ui.toast(n.text, 'error')));
  // Изменение в хранилище (здесь или в другой вкладке): перечитать окно, не чаще кадра.
  let refresh = 0;
  const offChange = props.files.onChange(() => {
    cancelAnimationFrame(refresh);
    refresh = requestAnimationFrame(() => request(true));
  });

  // ——— Превью ———
  const loadThumbs = (list: FileMeta[]) => {
    for (const m of list) {
      if (m.thumbVersion === 0 || thumbVersions.get(m.id) === m.thumbVersion) continue;
      thumbVersions.set(m.id, m.thumbVersion);
      props.files.thumbnail(m.id).then(
        (blob) => {
          if (!alive || thumbVersions.get(m.id) !== m.thumbVersion) return;
          const old = thumbs[m.id];
          if (old) URL.revokeObjectURL(old);
          if (blob) setThumbs(m.id, URL.createObjectURL(blob));
          else setThumbs(m.id, undefined!);
        },
        () => thumbVersions.delete(m.id),
      );
    }
    // Адреса превью далеко за окном освобождаются: память не растёт с прокруткой.
    const keep = new Set(list.map((m) => m.id));
    const ids = Object.keys(thumbs);
    if (ids.length > 3 * MAX_WINDOW) {
      for (const id of ids) {
        if (keep.has(id)) continue;
        URL.revokeObjectURL(thumbs[id]);
        setThumbs(id, undefined!);
        thumbVersions.delete(id);
      }
    }
  };

  onMount(() => {
    dismissSplash();
    // Размеры — из ResizeObserver (без чтения layout в обработчиках): ширина
    // списка задаёт столбцы, высота видимой области — окно записей.
    const ro = new ResizeObserver((entries) => {
      batch(() => {
        for (const e of entries) {
          if (e.target === list) setWidth(e.contentRect.width);
          else setHeight(e.contentRect.height);
        }
      });
    });
    ro.observe(list);
    ro.observe(scroller);
    const clock = setInterval(() => setNow(Date.now()), 30_000);
    onCleanup(() => {
      ro.disconnect();
      clearInterval(clock);
    });
  });
  onCleanup(() => {
    alive = false;
    offChange();
    cancelAnimationFrame(refresh);
    for (const url of Object.values(thumbs)) if (url) URL.revokeObjectURL(url);
  });

  let scrollFrame = 0;
  const onScroll = () => {
    if (scrollFrame) return;
    // Чтение прокрутки — в начале кадра, до записей в DOM.
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = 0;
      setScrollTop(scroller.scrollTop);
    });
  };
  onCleanup(() => cancelAnimationFrame(scrollFrame));

  // ——— Действия ———
  const open = (f: FileMeta) => props.navigate({ kind: 'file', id: f.id });
  const errorText = (what: string, e: unknown) => `${what}: ${e instanceof Error ? e.message : String(e)}`;
  const act = (what: string, p: Promise<unknown>) => p.catch((e: unknown) => ui.toast(errorText(what, e), 'error'));
  const download = async (f: FileMeta) => {
    try {
      const { bytes } = await props.files.read(f.id);
      if (!bytes) return;
      downloadBytes(fileName(f.name, '.ferrum'), new Uint8Array(bytes), 'application/octet-stream');
    } catch (e) {
      ui.toast(errorText('Не удалось скачать копию', e), 'error');
    }
  };
  const openInNewTab = (f: FileMeta) => {
    window.open(`${location.pathname}${location.search}#/file/${encodeURIComponent(f.id)}`, '_blank', 'noopener');
  };
  const importFile = async () => {
    const meta = await importFromDisk(props.files, (t) => ui.toast(t, 'error'));
    if (meta) open(meta);
  };
  const menuItems = (f: FileMeta): MenuItem[] =>
    props.section === 'trash'
      ? [
          { label: 'Восстановить', onSelect: () => void act('Не удалось восстановить файл', props.files.restore(f.id)) },
          { label: 'Удалить навсегда', onSelect: () => setConfirm({ kind: 'delete', file: f }) },
        ]
      : [
          { label: 'Открыть', onSelect: () => open(f) },
          { label: 'Открыть в новой вкладке', onSelect: () => openInNewTab(f) },
          { separator: true },
          { label: 'Переименовать', onSelect: () => setRenaming(f.id) },
          { label: 'Скачать копию (.ferrum)', disabled: f.size === 0, onSelect: () => void download(f) },
          { separator: true },
          { label: 'Удалить', onSelect: () => void act('Не удалось удалить файл', props.files.trash(f.id)) },
        ];

  // ——— Клавиатура списка: одна точка Tab, стрелки по сетке ———
  const itemAt = (i: number) => list.querySelector<HTMLElement>(`[data-index="${i}"]`);
  const focusIndex = (i: number) => {
    const n = total();
    if (n === 0) return;
    const next = Math.max(0, Math.min(n - 1, i));
    setFocused(next);
    const el = itemAt(next);
    if (el) {
      el.focus({ preventScroll: true });
      el.scrollIntoView({ block: 'nearest' });
      return;
    }
    // Записи нет в окне: прокрутить к ней, фокус — когда окно придёт.
    pendingFocus = next;
    const l = layout();
    scroller.scrollTop = Math.floor(next / l.cols) * l.rowH;
  };
  createEffect(
    on(
      () => rows.list,
      () => {
        if (pendingFocus === null) return;
        const el = itemAt(pendingFocus);
        if (el) {
          pendingFocus = null;
          el.focus({ preventScroll: true });
        }
      },
    ),
  );
  const fileAt = (i: number) => rows.list[i - rows.start];
  /** Точка Tab: выбранная запись, если она в окне, иначе первая в окне — у списка
   * всегда есть, куда прийти с клавиатуры. */
  const tabStop = () => (fileAt(focused()) ? focused() : rows.start);
  const onListKey = (e: KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement || e.altKey || e.ctrlKey || e.metaKey) return;
    const at = focused();
    const cols = layout().cols;
    const f = fileAt(at);
    switch (e.key) {
      case 'ArrowRight':
        focusIndex(at + 1);
        break;
      case 'ArrowLeft':
        focusIndex(at - 1);
        break;
      case 'ArrowDown':
        focusIndex(at + cols);
        break;
      case 'ArrowUp':
        focusIndex(at - cols);
        break;
      case 'Home':
        focusIndex(0);
        break;
      case 'End':
        focusIndex(total() - 1);
        break;
      case 'Enter':
        if (f && props.section !== 'trash') open(f);
        break;
      case 'F2':
        if (f && props.section !== 'trash') setRenaming(f.id);
        break;
      case 'Delete':
        if (f && props.section !== 'trash') void act('Не удалось удалить файл', props.files.trash(f.id));
        break;
      case 'ContextMenu':
        if (f) setMenu({ file: f, anchor: e.target as HTMLElement });
        break;
      default:
        if (e.key === 'F10' && e.shiftKey && f) {
          setMenu({ file: f, anchor: e.target as HTMLElement });
          break;
        }
        return;
    }
    e.preventDefault();
  };

  // Разделы — список с выбором (listbox): стрелки переходят к соседнему разделу.
  const onNavKey = (e: KeyboardEvent) => {
    const i = SECTIONS.findIndex((s) => s.id === props.section);
    let next = -1;
    if (e.key === 'ArrowDown') next = (i + 1) % SECTIONS.length;
    else if (e.key === 'ArrowUp') next = (i - 1 + SECTIONS.length) % SECTIONS.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = SECTIONS.length - 1;
    else return;
    e.preventDefault();
    props.navigate({ kind: 'files', section: SECTIONS[next].id });
  };
  // Раздел сменили стрелками: фокус переходит к выбранному пункту.
  createEffect(
    on(
      () => props.section,
      () => {
        if (nav.contains(document.activeElement)) nav.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
      },
      { defer: true },
    ),
  );

  const chooseView = (v: FilesView) => {
    setView(v);
    updateSettings(storage(), { filesView: v });
  };

  const sub = (f: FileMeta) =>
    props.section === 'trash'
      ? `Удалён ${relativeTime(f.trashedAt ?? 0, now())}`
      : `Изменён ${relativeTime(f.editedAt, now())}`;

  const empty = () =>
    loaded() && total() === 0 ? (query().trim() ? 'Ничего не найдено' : props.section === 'trash' ? 'Корзина пуста' : 'Здесь пока нет файлов') : '';

  const Thumb = (p: { file: FileMeta }): JSX.Element => (
    <div class="fe-card__thumb">
      <Show when={thumbs[p.file.id]} fallback={<Icon name="file" class="fe-card__placeholder" size={24} />}>
        {(url) => <img src={url()} alt="" draggable={false} />}
      </Show>
    </div>
  );

  const Name = (p: { file: FileMeta }): JSX.Element => (
    <Show
      when={renaming() === p.file.id}
      fallback={
        <span
          class="fe-card__name t-strong"
          title={p.file.name}
          onClick={(e) => e.stopPropagation()}
          onDblClick={(e) => {
            e.stopPropagation();
            if (props.section !== 'trash') setRenaming(p.file.id);
          }}
        >
          {p.file.name}
        </span>
      }
    >
      <RenameInput
        label="Имя файла"
        value={untrack(() => p.file.name)}
        onCommit={(name) => {
          if (name.trim() && name !== p.file.name) void act('Не удалось переименовать файл', props.files.rename(p.file.id, name));
        }}
        onClose={() => setRenaming(null)}
        returnFocus={() => itemAt(focused())}
      />
    </Show>
  );

  return (
    <div class="fe-files">
      <aside class="fe-files__side" aria-label="Файлы Ferrum">
        <div class="fe-files__brand">
          <span class="fe-panel-head__mark" aria-hidden="true" />
          <span class="t-strong">Ferrum</span>
        </div>
        <label class="fe-field fe-field--text fe-files__search">
          <Icon name="search" class="t-muted fe-field__icon" />
          <input
            type="search"
            placeholder="Поиск"
            aria-label="Поиск"
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setFocused(0);
              scroller.scrollTop = 0;
            }}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && e.currentTarget.value) {
                e.preventDefault();
                e.currentTarget.value = '';
                setQuery('');
              }
            }}
          />
        </label>
        <div ref={nav} class="fe-files__nav" role="listbox" aria-label="Разделы" onKeyDown={onNavKey}>
          <For each={SECTIONS}>
            {(s) => (
              <div
                class="fe-files__nav-item"
                role="option"
                aria-selected={props.section === s.id}
                tabindex={props.section === s.id ? 0 : -1}
                onClick={() => props.navigate({ kind: 'files', section: s.id })}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    props.navigate({ kind: 'files', section: s.id });
                  }
                }}
              >
                <Icon name={s.icon} />
                <span>{s.title}</span>
              </div>
            )}
          </For>
        </div>
      </aside>
      <main class="fe-files__main">
        <header class="fe-files__head">
          <h1 class="fe-files__title t-heading">{title()}</h1>
          <span class="fe-panel-head__spacer" />
          <Show
            when={props.section === 'trash'}
            fallback={
              <>
                <button class="fe-btn fe-btn--secondary" onClick={() => void importFile()}>
                  <Icon name="upload" />
                  Импорт…
                </button>
                <button class="fe-btn fe-btn--primary" onClick={() => props.navigate({ kind: 'new' })}>
                  <Icon name="plus" />
                  Новый файл
                </button>
              </>
            }
          >
            <button class="fe-btn fe-btn--secondary" disabled={total() === 0} onClick={() => setConfirm({ kind: 'empty' })}>
              Очистить корзину
            </button>
          </Show>
          <div class="fe-files__views" role="radiogroup" aria-label="Вид">
            <For each={[{ v: 'grid' as const, label: 'Сетка', icon: 'grid' as const }, { v: 'list' as const, label: 'Список', icon: 'list' as const }]}>
              {(o) => (
                <button
                  class="fe-tool"
                  role="radio"
                  aria-checked={view() === o.v}
                  aria-label={o.label}
                  title={o.label}
                  tabindex={view() === o.v ? 0 : -1}
                  onClick={() => chooseView(o.v)}
                  onKeyDown={(e) => {
                    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
                    e.preventDefault();
                    const next = view() === 'grid' ? 'list' : 'grid';
                    chooseView(next);
                    (e.currentTarget.parentElement?.querySelector(`[aria-label="${next === 'grid' ? 'Сетка' : 'Список'}"]`) as HTMLElement | null)?.focus();
                  }}
                >
                  <Icon name={o.icon} />
                </button>
              )}
            </For>
          </div>
        </header>
        <div ref={scroller} class="fe-files__scroll" onScroll={onScroll}>
          <Show when={empty()}>
            {(text) => (
              <div class="fe-files__empty">
                <p class="t-muted">{text()}</p>
                <Show when={text() === 'Здесь пока нет файлов'}>
                  <button class="fe-btn fe-btn--primary" onClick={() => props.navigate({ kind: 'new' })}>
                    <Icon name="plus" />
                    Новый файл
                  </button>
                </Show>
              </div>
            )}
          </Show>
          <div
            ref={list}
            class="fe-files__list"
            classList={{ 'fe-files__list--rows': view() === 'list' }}
            role="listbox"
            aria-label={title()}
            style={{ height: `${Math.ceil(total() / layout().cols) * layout().rowH}px` }}
            onKeyDown={onListKey}
          >
            <Index each={rows.list}>
              {(f, k) => {
                const index = () => rows.start + k;
                const pos = () => {
                  const l = layout();
                  const i = index();
                  const col = i % l.cols;
                  return { x: col * (l.cardW + GRID_GAP), y: Math.floor(i / l.cols) * l.rowH };
                };
                return (
                  <div
                    class={view() === 'list' ? 'fe-file-row' : 'fe-card'}
                    role="option"
                    data-index={index()}
                    aria-label={f().name}
                    aria-selected={focused() === index()}
                    tabindex={tabStop() === index() ? 0 : -1}
                    style={{
                      transform: `translate(${pos().x}px, ${pos().y}px)`,
                      width: `${layout().cardW}px`,
                      '--thumb-h': `${layout().thumbH}px`,
                    }}
                    onFocus={() => setFocused(index())}
                    onClick={() => (props.section === 'trash' ? setFocused(index()) : open(f()))}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setFocused(index());
                      setMenu({ file: f(), anchor: { x: e.clientX, y: e.clientY } });
                    }}
                  >
                    <Thumb file={f()} />
                    <div class="fe-card__meta">
                      <Name file={f()} />
                      <span class="fe-card__sub t-muted">{sub(f())}</span>
                      <Show when={view() === 'list'}>
                        <span class="fe-file-row__size t-muted t-num">{formatSize(f().size)}</span>
                      </Show>
                    </div>
                    <button
                      class="fe-tool fe-tool--small fe-card__more"
                      tabindex="-1"
                      aria-label="Действия с файлом"
                      aria-haspopup="menu"
                      onClick={(e) => {
                        e.stopPropagation();
                        setFocused(index());
                        setMenu({ file: f(), anchor: e.currentTarget });
                      }}
                    >
                      <Icon name="more" />
                    </button>
                  </div>
                );
              }}
            </Index>
          </div>
        </div>
      </main>
      <Show when={menu()}>
        {(m) => <Menu label="Действия с файлом" items={menuItems(m().file)} anchor={m().anchor} onClose={() => setMenu(null)} />}
      </Show>
      <Show when={confirm()}>
        {(c) => {
          const v = c();
          return v.kind === 'delete' ? (
            <ConfirmDialog
              title="Удалить файл навсегда?"
              text={`Файл «${v.file.name}» будет удалён без возможности восстановления`}
              confirm="Удалить"
              cancel="Отмена"
              onCancel={() => setConfirm(null)}
              onConfirm={() => {
                setConfirm(null);
                void act('Не удалось удалить файл', props.files.remove(v.file.id));
              }}
            />
          ) : (
            <ConfirmDialog
              title="Очистить корзину?"
              text="Все файлы в корзине будут удалены без возможности восстановления"
              confirm="Очистить"
              cancel="Отмена"
              onCancel={() => setConfirm(null)}
              onConfirm={() => {
                setConfirm(null);
                void act('Не удалось очистить корзину', props.files.emptyTrash());
              }}
            />
          );
        }}
      </Show>
      <Toasts store={ui} />
    </div>
  );
}
