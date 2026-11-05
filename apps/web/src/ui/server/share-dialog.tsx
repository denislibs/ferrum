// «Поделиться» (спека подсистемы 4, раздел 6.3): приглашение по email с ролью, список людей, смена и
// снятие роли, ссылка с ролью и выключателем. Читателю кнопки нет: список людей и адреса ему не положены.

import { MAX_EMAIL_BYTES, parseEmail, type ShareDto, type ShareRole } from '@ferrum/protocol';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import type { FileMeta } from '../../core/file-list';
import { Modal } from './modal';
import { apiFailureText, changeRole, getShare, invitePerson, removePerson, resetLink, updateLink, type ApiResult } from './server-api';

const ROLE_TEXT: Record<string, string> = { owner: 'Владелец', editor: 'Может редактировать', viewer: 'Может смотреть' };

export function ShareButton(props: { file: FileMeta; role: () => FileMeta['role']; canShare: () => boolean }) {
  const [open, setOpen] = createSignal(false);
  return (
    <Show when={props.role() !== 'viewer' && props.canShare()}>
      <button class="fe-btn fe-btn--primary" onClick={() => setOpen(true)}>
        Поделиться
      </button>
      <Show when={open()}>
        <ShareDialog file={props.file} onClose={() => setOpen(false)} />
      </Show>
    </Show>
  );
}

function ShareDialog(props: { file: FileMeta; onClose: () => void }) {
  const id = props.file.id;
  const [state, setState] = createSignal<ShareDto | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [email, setEmail] = createSignal('');
  const [role, setRole] = createSignal<ShareRole>('editor');
  const [copied, setCopied] = createSignal(false);
  /** Сервер не даёт этому человеку звать людей: повторять нечего, и форма ему ни к чему. */
  const [refused, setRefused] = createSignal(false);
  /** Поколение: ответ на прежний запрос не затирает то, что уже пришло по новому. */
  let generation = 0;
  let closed = false;
  let copyTimer = 0;
  onCleanup(() => {
    closed = true;
    clearTimeout(copyTimer);
  });

  async function run(work: () => Promise<ApiResult<ShareDto>>): Promise<boolean> {
    const my = ++generation;
    setBusy(true);
    setError(null);
    const r = await work();
    if (closed || my !== generation) return false;
    setBusy(false);
    if (!r.ok) {
      if (r.failure === 'FORBIDDEN' && !state()) setRefused(true);
      else setError(apiFailureText(r.failure));
      return false;
    }
    setState(r.value);
    return true;
  }
  const load = () => void run(() => getShare(id));
  load();

  const invite = async (e: Event) => {
    e.preventDefault();
    const parsed = parseEmail(email());
    if (!parsed) {
      setError('Проверьте адрес');
      return;
    }
    if (await run(() => invitePerson(id, parsed, role()))) setEmail('');
  };
  const remove = (entry: string) =>
    void run(async () => {
      const r = await removePerson(id, entry);
      if (!r.ok) return r;
      return getShare(id);
    });
  const linkUrl = () => {
    const token = state()?.link?.token;
    return token ? `${location.origin}${location.pathname}#/file/${encodeURIComponent(id)}?link=${encodeURIComponent(token)}` : null;
  };
  const copy = async () => {
    const url = linkUrl();
    if (!url) return;
    await navigator.clipboard.writeText(url).then(
      () => {
        setCopied(true);
        clearTimeout(copyTimer);
        copyTimer = window.setTimeout(() => setCopied(false), 2000);
      },
      () => setError('Браузер не дал скопировать ссылку'),
    );
  };

  return (
    <Modal title={`Поделиться: ${props.file.name}`} onClose={props.onClose}>
      <Show when={state()}>
        <form class="fe-share__invite" novalidate onSubmit={(e) => void invite(e)}>
          <div class="fe-field fe-field--text fe-share__email">
            <input
              ref={(el) => queueMicrotask(() => el.focus())}
              type="email"
              aria-label="Адрес, кого пригласить"
              placeholder="Электронная почта"
              autocomplete="off"
              maxlength={MAX_EMAIL_BYTES}
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
            />
          </div>
          <select
            class="fe-share__select"
            aria-label="Роль приглашённого"
            value={role()}
            onChange={(e) => setRole(e.currentTarget.value as ShareRole)}
          >
            <option value="editor">{ROLE_TEXT.editor}</option>
            <option value="viewer">{ROLE_TEXT.viewer}</option>
          </select>
          <button class="fe-btn fe-btn--primary" type="submit" disabled={busy()}>
            Пригласить
          </button>
        </form>
      </Show>
      <Show when={refused()}>
        <p class="t-muted">Делиться файлом может тот, кому его открыли напрямую</p>
      </Show>
      <Show when={error()}>
        <p class="fe-share__error" role="alert">
          {error()}
          <Show when={!state()}>
            {' '}
            <button class="fe-btn fe-btn--secondary" onClick={load}>
              Повторить
            </button>
          </Show>
        </p>
      </Show>
      <Show
        when={state()}
        fallback={
          <Show when={!error() && !refused()}>
            <p class="t-muted">Загрузка…</p>
          </Show>
        }
      >
        {(s) => (
          <>
            <ul class="fe-share__people" aria-label="Люди с доступом">
              <For each={s().people}>
                {(p) => (
                  <li class="fe-share__person">
                    <span class="fe-share__who">
                      <Show when={p.name}>
                        <span class="t-strong">{p.name}</span>
                      </Show>
                      <span class="t-muted">{p.email}</span>
                    </span>
                    <Show when={s().canManage && p.role !== 'owner'} fallback={<span class="t-muted">{ROLE_TEXT[p.role]}</span>}>
                      <select
                        class="fe-share__select"
                        aria-label={`Роль: ${p.email}`}
                        value={p.role}
                        disabled={busy()}
                        onChange={(e) => {
                          const el = e.currentTarget;
                          // Отказ: на экране остаётся то, что на сервере.
                          void run(() => changeRole(id, p.id, el.value as ShareRole)).then((ok) => {
                            if (!ok) el.value = p.role;
                          });
                        }}
                      >
                        <option value="editor">{ROLE_TEXT.editor}</option>
                        <option value="viewer">{ROLE_TEXT.viewer}</option>
                      </select>
                      <button class="fe-btn" aria-label={`Снять доступ: ${p.email}`} disabled={busy()} onClick={() => remove(p.id)}>
                        Снять
                      </button>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
            <Show when={s().canManage}>
              <div class="fe-share__link">
                <label class="fe-share__toggle">
                  <input
                    type="checkbox"
                    checked={s().link?.enabled ?? false}
                    disabled={busy()}
                    onChange={(e) => {
                      const el = e.currentTarget;
                      void run(() => updateLink(id, el.checked, s().link?.role ?? 'viewer')).then((ok) => {
                        if (!ok) el.checked = s().link?.enabled ?? false;
                      });
                    }}
                  />
                  Доступ по ссылке
                </label>
                <Show when={s().link?.enabled}>
                  <select
                    class="fe-share__select"
                    aria-label="Роль по ссылке"
                    value={s().link?.role}
                    disabled={busy()}
                    onChange={(e) => {
                      const el = e.currentTarget;
                      void run(() => updateLink(id, true, el.value as ShareRole)).then((ok) => {
                        if (!ok) el.value = s().link?.role ?? 'viewer';
                      });
                    }}
                  >
                    <option value="editor">{ROLE_TEXT.editor}</option>
                    <option value="viewer">{ROLE_TEXT.viewer}</option>
                  </select>
                  <Show when={linkUrl()} fallback={<span class="t-muted">Ссылка больше не действует, создайте новую</span>}>
                    <button class="fe-btn fe-btn--secondary" onClick={() => void copy()}>
                      {copied() ? 'Скопировано' : 'Копировать ссылку'}
                    </button>
                  </Show>
                  <button class="fe-btn" disabled={busy()} onClick={() => void run(() => resetLink(id))}>
                    Новая ссылка
                  </button>
                </Show>
              </div>
            </Show>
          </>
        )}
      </Show>
      <div class="fe-dialog__actions">
        <button class="fe-btn" onClick={props.onClose}>
          Закрыть
        </button>
      </div>
    </Modal>
  );
}
