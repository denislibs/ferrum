// Файлы прежней версии из IndexedDB (спека подсистемы 4, раздел 6.5): «Локальные файлы (N)» с переносом на
// сервер по одному или все сразу. Оригинал остаётся, пока его не удалят. Байты идут из worker'а в worker
// передачей, основной поток их не читает; журнал правок переносится вместе с файлом.

import { createSignal, For, onCleanup, Show } from 'solid-js';
import { MAX_WINDOW, type FileMeta } from '../../core/file-list';
import { Modal } from './modal';
import type { ServerFileStore } from './server-store';

const movedKey = (user: string) => `ferrum-moved-files:${user}`;

function readMoved(user: string): Set<string> {
  try {
    const v = JSON.parse(localStorage.getItem(movedKey(user)) ?? '[]') as unknown;
    return new Set(Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

export function LocalFilesEntry(props: { store: () => ServerFileStore | null; user: string }) {
  const [count, setCount] = createSignal(0);
  const [open, setOpen] = createSignal(false);
  let closed = false;
  onCleanup(() => (closed = true));
  const refresh = () => {
    const s = props.store();
    if (!s) return;
    s.local.list('drafts', '', 0, 1).then(
      (r) => !closed && setCount(r.total),
      () => {},
    );
  };
  refresh();
  return (
    <Show when={count() > 0}>
      <button class="fe-btn fe-files__extra-btn" onClick={() => setOpen(true)}>
        Локальные файлы ({count()})
      </button>
      <Show when={open() && props.store()}>
        {(s) => (
          <LocalFilesDialog
            store={s()}
            user={props.user}
            onClose={() => {
              setOpen(false);
              refresh();
            }}
          />
        )}
      </Show>
    </Show>
  );
}

function LocalFilesDialog(props: { store: ServerFileStore; user: string; onClose: () => void }) {
  const [rows, setRows] = createSignal<FileMeta[]>([]);
  const [total, setTotal] = createSignal(0);
  const [moved, setMoved] = createSignal(readMoved(props.user));
  const [status, setStatus] = createSignal<Record<string, string>>({});
  const [running, setRunning] = createSignal(false);
  let closed = false;
  onCleanup(() => (closed = true));
  const load = () =>
    props.store.local.list('drafts', '', 0, MAX_WINDOW).then(
      (r) => {
        if (closed) return;
        setRows(r.rows);
        setTotal(r.total);
      },
      (e: unknown) => !closed && setStatus({ '': `Не удалось прочитать файлы браузера: ${e instanceof Error ? e.message : String(e)}` }),
    );
  void load();
  const note = (id: string, text: string) => setStatus((s) => ({ ...s, [id]: text }));
  const moveOne = async (f: FileMeta): Promise<void> => {
    note(f.id, 'Переносится…');
    try {
      const r = await props.store.local.read(f.id);
      await props.store.importLocal(f.name, r.bytes, r.bytes ? r.journal : []);
      if (closed) return;
      const next = new Set(moved()).add(f.id);
      setMoved(next);
      try {
        localStorage.setItem(movedKey(props.user), JSON.stringify([...next]));
      } catch {
        // Отметка «перенесён» — только удобство: без неё перенос можно повторить.
      }
      note(f.id, 'Перенесён в «Черновики»');
    } catch (e) {
      if (!closed) note(f.id, `Не перенесён: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  const moveAll = async () => {
    setRunning(true);
    // По одному: каждый файл может быть большим.
    for (const f of rows()) {
      if (closed) return;
      if (!moved().has(f.id)) await moveOne(f);
    }
    if (!closed) setRunning(false);
  };
  const remove = async (f: FileMeta) => {
    try {
      await props.store.local.trash(f.id);
      await props.store.local.remove(f.id);
      if (!closed) await load();
    } catch (e) {
      if (!closed) note(f.id, `Не удалён: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  return (
    <Modal title={`Локальные файлы (${total()})`} onClose={props.onClose}>
      <p class="t-muted">
        Эти файлы лежат только в этом браузере. Перенос создаёт копию в «Черновиках», оригинал остаётся, пока вы его не удалите.
      </p>
      <Show when={status()['']}>
        {(text) => (
          <p class="fe-share__error" role="alert">
            {text()}
          </p>
        )}
      </Show>
      <ul class="fe-share__people" aria-label="Файлы в браузере">
        <For each={rows()}>
          {(f) => (
            <li class="fe-share__person">
              <span class="fe-share__who">
                <span class="t-strong">{f.name}</span>
                <Show when={status()[f.id] ?? (moved().has(f.id) ? 'Перенесён в «Черновики»' : '')}>
                  {(t) => <span class="t-muted">{t()}</span>}
                </Show>
              </span>
              <button
                class="fe-btn fe-btn--secondary"
                aria-label={`Перенести: ${f.name}`}
                disabled={running()}
                onClick={() => void moveOne(f)}
              >
                Перенести
              </button>
              <button class="fe-btn" aria-label={`Удалить из браузера: ${f.name}`} disabled={running()} onClick={() => void remove(f)}>
                Удалить
              </button>
            </li>
          )}
        </For>
      </ul>
      <div class="fe-dialog__actions">
        <button class="fe-btn" onClick={props.onClose}>
          Закрыть
        </button>
        <button
          class="fe-btn fe-btn--primary"
          disabled={running() || rows().every((f) => moved().has(f.id))}
          onClick={() => void moveAll()}
        >
          Перенести все
        </button>
      </div>
    </Modal>
  );
}
