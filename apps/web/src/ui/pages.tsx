// Список страниц (сворачиваемый): добавить, переименовать на месте, удалить,
// переключить; контекстное меню строки (из ядра) — переименовать, дублировать,
// удалить, ссылка на страницу. Операции со страницами в историю отмены не
// попадают, поэтому удаление — только после подтверждения.

import { createSignal, Index, Show, untrack } from 'solid-js';
import type { PageInfo } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { ConfirmDialog } from './dialog';
import { Icon } from './icons';
import { editableTarget } from './keys';
import { RenameInput } from './rename-input';
import { routeUrl } from './router';

/** Ссылка на страницу файла: открывает файл на этой странице. */
export function pageLink(fileId: string, pageId: string): string {
  return `${location.origin}${routeUrl({ kind: 'file', id: fileId, page: pageId })}`;
}

export function PagesPanel(props: { open: boolean; onToggle: (open: boolean) => void; fileId: string }) {
  const ed = useEditor();
  const pages = () => ed.store.state.pages;
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [deleting, setDeleting] = createSignal<PageInfo | null>(null);
  const askDelete = (p: PageInfo) => {
    if (pages().list.length > 1) setDeleting(p);
  };
  /** Имя на момент открытия поля, по id страницы (строка списка могла смениться). */
  const rename = (id: string, original: string, name: string) => {
    if (name.trim() && name !== original) ed.send({ type: 'renamePage', id, name });
  };
  const copyLink = (id: string) => {
    const fail = () => ed.store.toast('Не удалось записать ссылку в буфер обмена: браузер не дал доступа', 'error');
    // Небезопасный контекст: `navigator.clipboard` нет — то же уведомление, без исключения.
    const clip = (globalThis as { navigator?: { clipboard?: Clipboard } }).navigator?.clipboard;
    if (typeof clip?.writeText !== 'function') return fail();
    clip.writeText(pageLink(props.fileId, id)).then(() => ed.store.toast('Ссылка на страницу скопирована'), fail);
  };
  /** Контекстное меню строки: пункты и их доступность — из ядра. */
  const openMenu = (p: PageInfo, e: MouseEvent) => {
    // В поле переименования — родное меню поля (вырезать, вставить).
    if (editableTarget(e.target)) return;
    e.preventDefault();
    ed.contextMenu?.({ kind: 'page', id: p.id }, { x: e.clientX, y: e.clientY }, 'Действия со страницей', (a) => {
      if (a === 'renamePage') setRenaming(p.id);
      else if (a === 'duplicatePage') ed.send({ type: 'duplicatePage', id: p.id });
      else if (a === 'deletePage') askDelete(p);
      else if (a === 'copyPageLink') copyLink(p.id);
    });
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
              let li!: HTMLLIElement;
              return (
                <li
                  ref={li}
                  class="fe-page"
                  role="option"
                  aria-selected={current()}
                  tabindex={current() ? 0 : -1}
                  onClick={() => !current() && ed.send({ type: 'setPage', id: page().id })}
                  onDblClick={() => setRenaming(page().id)}
                  onContextMenu={(e) => openMenu(page(), e)}
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
                    {(() => {
                      const { id, name } = untrack(page);
                      return (
                        <RenameInput
                          label="Имя страницы"
                          value={name}
                          onCommit={(typed) => rename(id, name, typed)}
                          onClose={() => renaming() === id && setRenaming(null)}
                          returnFocus={() => li}
                        />
                      );
                    })()}
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
