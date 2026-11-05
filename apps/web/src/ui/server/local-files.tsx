// Файлы прежней версии из IndexedDB (спека подсистемы 4, раздел 6.5): «Локальные файлы (N)» с переносом на
// сервер по одному или все сразу. Оригинал остаётся, пока его не удалят. Байты идут из worker'а браузера через
// основной поток в worker сервера передачей, без копий, и основной поток их не читает; журнал правок
// переносится вместе с файлом.

import { createSignal, For, onCleanup, Show } from 'solid-js';
import { MAX_WINDOW, type FileMeta } from '../../core/file-list';
import { MAX_OPEN_BYTES } from '../../core/limits';
import { ConfirmDialog } from '../dialog';
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

// Перенос живёт дольше диалога: закрыли и открыли заново, пока файл идёт, — строка знает, что он в пути.
const [moved, setMoved] = createSignal<ReadonlySet<string>>(new Set());
const [moving, setMoving] = createSignal<ReadonlySet<string>>(new Set());
const [status, setStatus] = createSignal<Record<string, string>>({});
/** Журнал не помещается в перенос: файл можно перенести только явно, без правок. */
const [tooLarge, setTooLarge] = createSignal<ReadonlySet<string>>(new Set());
let stateUser: string | null = null;

const note = (id: string, text: string) => setStatus((s) => ({ ...s, [id]: text }));
const withId = (set: ReadonlySet<string>, id: string) => new Set(set).add(id);
const withoutId = (set: ReadonlySet<string>, id: string) => {
  const next = new Set(set);
  next.delete(id);
  return next;
};

async function moveOne(store: ServerFileStore, user: string, f: FileMeta, dropEdits = false): Promise<void> {
  if (moving().has(f.id)) return;
  setMoving((s) => withId(s, f.id));
  note(f.id, 'Переносится…');
  try {
    const r = await store.local.read(f.id);
    const size = r.journal.reduce((n, b) => n + b.byteLength, r.bytes?.byteLength ?? 0);
    // Такой журнал потом не применится: перенос без правок выглядел бы удачным, а правки пропали бы.
    if (!dropEdits && r.bytes && (r.journalTooLarge || size > MAX_OPEN_BYTES)) {
      setTooLarge((s) => withId(s, f.id));
      note(f.id, 'Не перенесён: несохранённых правок слишком много, вместе с файлом они не перенесутся');
      return;
    }
    await store.importLocal(f.name, r.bytes, r.bytes && !dropEdits ? r.journal : []);
    if (dropEdits) {
      // Правки остались только здесь, поэтому отметки «перенесён» нет.
      note(f.id, 'Копия без несохранённых правок — в «Черновиках». Правки остались в этом браузере');
      return;
    }
    const next = withId(moved(), f.id);
    setMoved(next);
    setTooLarge((s) => withoutId(s, f.id));
    try {
      localStorage.setItem(movedKey(user), JSON.stringify([...next]));
    } catch {
      // Отметка «перенесён» — только удобство: без неё перенос можно повторить.
    }
    note(f.id, 'Перенесён в «Черновики»');
  } catch (e) {
    note(f.id, `Не перенесён: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    setMoving((s) => withoutId(s, f.id));
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
  const [running, setRunning] = createSignal(false);
  const [removing, setRemoving] = createSignal<FileMeta | null>(null);
  let closed = false;
  onCleanup(() => (closed = true));
  // Вошёл другой человек без перезагрузки: чужие отметки ему не показываются.
  if (stateUser !== props.user) {
    stateUser = props.user;
    setStatus({});
    setTooLarge(new Set<string>());
  }
  setMoved(readMoved(props.user));
  const load = () =>
    props.store.local.list('drafts', '', 0, MAX_WINDOW).then(
      (r) => {
        if (closed) return;
        setRows(r.rows);
        setTotal(r.total);
      },
      (e: unknown) => !closed && note('', `Не удалось прочитать файлы браузера: ${e instanceof Error ? e.message : String(e)}`),
    );
  void load();
  const move = (f: FileMeta, dropEdits = false) => moveOne(props.store, props.user, f, dropEdits);
  const moveAll = async () => {
    setRunning(true);
    // По одному: каждый файл может быть большим. Закрыли диалог — идущий докатывается, следующие не начинаются.
    for (const f of rows()) {
      if (closed) return;
      if (!moved().has(f.id)) await move(f);
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
    <>
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
                <Show when={tooLarge().has(f.id) && !moved().has(f.id)}>
                  <button
                    class="fe-btn"
                    aria-label={`Перенести без правок: ${f.name}`}
                    disabled={running() || moving().has(f.id)}
                    onClick={() => void move(f, true)}
                  >
                    Перенести без правок
                  </button>
                </Show>
                <button
                  class="fe-btn fe-btn--secondary"
                  aria-label={`Перенести: ${f.name}`}
                  disabled={running() || moving().has(f.id) || moved().has(f.id)}
                  onClick={() => void move(f)}
                >
                  Перенести
                </button>
                <button
                  class="fe-btn"
                  aria-label={`Удалить из браузера: ${f.name}`}
                  disabled={running() || moving().has(f.id)}
                  onClick={() => setRemoving(f)}
                >
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
      <Show when={removing()}>
        {(f) => (
          <ConfirmDialog
            title="Удалить файл из браузера?"
            text={
              moved().has(f().id)
                ? `Копия «${f().name}» есть в «Черновиках», а этот оригинал будет удалён без возможности восстановления`
                : `Файл «${f().name}» не перенесён и есть только в этом браузере. Он будет удалён без возможности восстановления`
            }
            confirm="Удалить"
            cancel="Отмена"
            onCancel={() => setRemoving(null)}
            onConfirm={() => {
              const file = f();
              setRemoving(null);
              void remove(file);
            }}
          />
        )}
      </Show>
    </>
  );
}
