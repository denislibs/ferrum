// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить. Операции со страницами в историю отмены не попадают, поэтому
// удаление — только после подтверждения.

import { createSignal, Index, Show, untrack } from 'solid-js';
import type { PageInfo } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { ConfirmDialog } from './dialog';
import { Icon } from './icons';
import { MAX_NAME_CHARS } from '../core/limits';

export function PagesPanel(props: { open: boolean; onToggle: (open: boolean) => void }) {
  const ed = useEditor();
  const pages = () => ed.store.state.pages;
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [deleting, setDeleting] = createSignal<PageInfo | null>(null);
  const askDelete = (p: PageInfo) => {
    if (pages().list.length > 1) setDeleting(p);
  };
  const rename = (p: PageInfo, name: string) => {
    // Esc уже закрыл поле: blur удаляемого поля не должен применять набранное.
    if (renaming() !== p.id) return;
    setRenaming(null);
    if (name.trim() && name !== p.name) ed.send({ type: 'renamePage', id: p.id, name });
  };
  return (
    <section class="fe-pages" aria-label="Страницы">
      <div class="fe-section__head fe-pages__head">
        <button
          class="fe-pages__toggle"
          aria-expanded={props.open}
          aria-controls="fe-pages-list"
          onClick={() => props.onToggle(!props.open)}
        >
          <Icon name="expand" size={12} class={props.open ? 'fe-rot90' : ''} />
          <h2 class="fe-section__title">Страницы</h2>
        </button>
        <button
          class="fe-tool fe-tool--small"
          aria-label="Добавить страницу"
          data-tip="Добавить страницу"
          onClick={() => ed.send({ type: 'addPage' })}
        >
          <Icon name="plus" />
        </button>
      </div>
      <Show when={props.open}>
        <ul class="fe-pages__list" id="fe-pages-list" role="listbox" aria-label="Страницы">
          {/* Index: событие страниц присылает новые объекты, и For пересоздавал бы строки
            вместе с открытым полем имени. */}
          <Index each={pages().list}>
            {(page) => {
              const current = () => page().id === pages().current;
              return (
                <li
                  class="fe-page"
                  role="option"
                  aria-selected={current()}
                  tabindex={current() ? 0 : -1}
                  onClick={() => !current() && ed.send({ type: 'setPage', id: page().id })}
                  onDblClick={() => setRenaming(page().id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') ed.send({ type: 'setPage', id: page().id });
                    else if (e.key === 'F2') setRenaming(page().id);
                    else if (e.key === 'Delete' || e.key === 'Backspace') askDelete(page());
                    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                      const items = Array.from((e.currentTarget.parentElement as HTMLElement).children) as HTMLElement[];
                      items[items.indexOf(e.currentTarget) + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
                    } else return;
                    e.preventDefault();
                  }}
                >
                  <span class="fe-page__check">
                    <Show when={current()}>
                      <Icon name="check" />
                    </Show>
                  </span>
                  <Show when={renaming() === page().id} fallback={<span class="fe-page__name">{page().name}</span>}>
                    <input
                      class="fe-rename"
                      aria-label="Имя страницы"
                      maxlength={MAX_NAME_CHARS}
                      value={untrack(() => page().name)}
                      ref={(el) =>
                        queueMicrotask(() => {
                          el.focus();
                          el.select();
                        })
                      }
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => {
                        e.stopPropagation();
                        if (e.key === 'Enter') rename(page(), e.currentTarget.value);
                        if (e.key === 'Escape') setRenaming(null);
                      }}
                      onBlur={(e) => rename(page(), e.currentTarget.value)}
                    />
                  </Show>
                  <button
                    class="fe-tool fe-tool--small fe-tool--muted fe-page__delete"
                    aria-label={`Удалить страницу «${page().name}»`}
                    aria-disabled={pages().list.length <= 1 ? 'true' : undefined}
                    data-tip={pages().list.length <= 1 ? 'Единственную страницу удалить нельзя' : 'Удалить страницу'}
                    tabindex="-1"
                    onClick={(e) => {
                      e.stopPropagation();
                      askDelete(page());
                    }}
                  >
                    <Icon name="trash" />
                  </button>
                </li>
              );
            }}
          </Index>
        </ul>
      </Show>
      <Show when={deleting()}>
        {(p) => (
          <ConfirmDialog
            title="Удалить страницу?"
            text={`Страница «${p().name}» будет удалена вместе со всем, что на ней. Отменить это нельзя`}
            confirm="Удалить"
            cancel="Отмена"
            onCancel={() => setDeleting(null)}
            onConfirm={() => {
              ed.send({ type: 'deletePage', id: p().id });
              setDeleting(null);
            }}
          />
        )}
      </Show>
    </section>
  );
}
