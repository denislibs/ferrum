// Человек в интерфейсе: кружок с инициалами, вкладка настроек и строка на экране файлов.

import { createSignal, Show } from 'solid-js';
import { avatarHue, initials } from './auth-logic';
import { currentUser, leave } from './session';
import type { SessionUser } from '@ferrum/protocol';

export function Avatar(props: { user: SessionUser }) {
  return (
    <span class="fe-avatar" style={{ '--avatar-hue': avatarHue(props.user.id) }} aria-hidden="true">
      {initials(props.user)}
    </span>
  );
}

/** Вкладка «Аккаунт» настроек: кто вошёл, выход на этом устройстве и на всех. */
export function AccountTab() {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const out = async (everywhere: boolean) => {
    setBusy(true);
    setError(await leave(everywhere));
    setBusy(false);
  };
  return (
    <div class="fe-account">
      <Show when={currentUser()}>
        {(u) => (
          <div class="fe-account__who">
            <Avatar user={u()} />
            <span class="fe-account__text">
              <span class="t-strong">{u().name}</span>
              <span class="t-muted">{u().email}</span>
            </span>
          </div>
        )}
      </Show>
      <div class="fe-account__actions">
        <button class="fe-btn fe-btn--secondary" disabled={busy()} onClick={() => void out(false)}>
          Выйти
        </button>
        <button class="fe-btn fe-btn--secondary" disabled={busy()} onClick={() => void out(true)}>
          Выйти на всех устройствах
        </button>
      </div>
      <Show when={error()}>
        <p class="fe-auth__error" role="alert">
          {error()}
        </p>
      </Show>
    </div>
  );
}

/** Строка внизу панели экрана файлов (поведение: настройки открываются только из редактора,
 * а выйти нужно и отсюда). */
export function AccountRow() {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const out = async () => {
    setBusy(true);
    setError(await leave(false));
    setBusy(false);
  };
  return (
    <Show when={currentUser()}>
      {(u) => (
        <div class="fe-account-row">
          <Avatar user={u()} />
          <span class="fe-account-row__name" title={u().email}>
            {u().name}
          </span>
          <button class="fe-btn" disabled={busy()} onClick={() => void out()}>
            Выйти
          </button>
          <Show when={error()}>
            <p class="fe-auth__error" role="alert">
              {error()}
            </p>
          </Show>
        </div>
      )}
    </Show>
  );
}
