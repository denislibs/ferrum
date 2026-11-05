// Обязательный вход, как в Figma: без сессии открывается только экран входа, запрошенный адрес
// остаётся в адресной строке и уходит в письмо. Приложение монтируется только при живой сессии.
// Весь этот код есть только в рабочей сборке (см. `main.tsx`, `scripts/check-dist.mjs`).

import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch, type JSX } from 'solid-js';
import { MAX_NAME_CHARS, parseConfirmHash, parseEmail, parseNext, type SessionUser } from '@ferrum/protocol';
import { dismissSplash } from '../boot';
import { editorHooks } from '../extensions';
import { confirmLink, getSession, requestLink, saveName } from './auth-api';
import { failureText, resendLeft, screenFor, type AuthFailure, type AuthScreen } from './auth-logic';
import { announceAuthChange, currentUser, onAuthChange, setCurrentUser } from './session';

export function AuthGate(props: { children: JSX.Element }) {
  const [screen, setScreen] = createSignal<AuthScreen | null>(null);
  const [failure, setFailure] = createSignal<AuthFailure | null>(null);
  // Экран не пересоздаётся от новой загрузки: новая ссылка — новая страница подтверждения, а тот же
  // адрес или тот же человек — прежний экран с набранным текстом.
  const confirm = createMemo(
    () => {
      const s = screen();
      return s?.kind === 'confirm' ? s : null;
    },
    null,
    { equals: (a, b) => a?.token === b?.token && a?.next === b?.next },
  );
  const naming = createMemo(
    () => {
      const s = screen();
      return s?.kind === 'name' ? s.user : null;
    },
    null,
    { equals: (a, b) => a?.id === b?.id },
  );

  /** Поколение загрузки: ответ для прежнего адреса не заменяет то, что уже открылось по новому. */
  let generation = 0;
  /** Ошибка остаётся на экране, пока повтор не удался: карточка не мигает пустой страницей.
   * `keepShown` — фоновое обновление показанного экрана: его сбой этот экран не снимает. */
  const load = async (keepShown = false) => {
    const my = ++generation;
    const hash = location.hash;
    // Страница подтверждения не спрашивает сессию: открытие ссылки ни к чему не обращается.
    const first = screenFor(hash, null);
    if (first.kind === 'confirm') {
      setFailure(null);
      setScreen(first);
      return;
    }
    let r: Awaited<ReturnType<typeof getSession>>;
    try {
      r = await getSession();
    } catch {
      // Разбор ответа уже сводит всё к `NETWORK`; сюда попадает только невозможное. Сбой монтирования
      // экрана здесь не ловится: он не «сервер недоступен», и заставка при нём остаётся (B-235).
      r = { ok: false, failure: 'NETWORK' };
    }
    if (my !== generation) return;
    if (!r.ok) {
      if (!keepShown) setFailure(r.failure);
      return;
    }
    setCurrentUser(r.value.user);
    setFailure(null);
    setScreen(screenFor(hash, r.value.user));
  };
  const [retrying, setRetrying] = createSignal(false);
  const retry = () => {
    setRetrying(true);
    void load().finally(() => setRetrying(false));
  };

  /** Новый адрес — без перезагрузки: приложение прочтёт его при монтировании. */
  const go = (hash: string) => {
    history.replaceState(null, '', `${location.pathname}${location.search}${hash}`);
    void load();
  };
  const signedIn = (next: string | null) => {
    announceAuthChange();
    go(next ?? '#/');
  };
  const named = (user: SessionUser) => {
    generation++;
    announceAuthChange();
    setCurrentUser(user);
    setScreen({ kind: 'app', user });
  };

  void load();
  // Ссылка из письма вставлена в эту же вкладку: сменился только адрес после `#`, страница не
  // перезагружается. Внутри приложения адресом ведает оно само (ссылку из письма оно пропускает,
  // см. `extensions.ownsHash`), а ссылку подтверждения открывает страница заново: приложение не
  // снимается посреди записи, сначала редактор дописывает файл. Не вышло и человек остался — адрес
  // возвращается.
  const onHash = (e: HashChangeEvent) => {
    const kind = screen()?.kind;
    if (kind !== 'app') {
      void load(kind === 'login' || kind === 'name');
      return;
    }
    if (!parseConfirmHash(location.hash)) return;
    const back = new URL(e.oldURL).hash;
    void (editorHooks.leave?.() ?? Promise.resolve(true)).then((ok) => {
      if (ok) location.reload();
      else history.replaceState(history.state, '', `${location.pathname}${location.search}${back}`);
    });
  };
  window.addEventListener('hashchange', onHash);
  onCleanup(() => window.removeEventListener('hashchange', onHash));
  // Вход или выход в другой вкладке: здесь то же самое. Страница заново — только если человек
  // или его имя правда другие: редактор не закрывается от повторного входа тем же адресом.
  onCleanup(
    onAuthChange(() => {
      if (screen()?.kind === 'confirm') return;
      void getSession().then(async (r) => {
        if (!r.ok) return;
        const was = currentUser();
        const now = r.value.user;
        if (was?.id === now?.id && was?.name === now?.name) return;
        // Спросить человека некому (он в другой вкладке): файл записывается молча.
        const saved = await (editorHooks.flush?.() ?? Promise.resolve(true)).catch(() => false);
        // Сессии уже нет, файл не записался: перезагрузка потеряла бы то, чего нет в журнале.
        if (saved || now !== null) location.reload();
        else editorHooks.sessionLost?.();
      });
    }),
  );

  return (
    <Switch>
      <Match when={failure()}>
        {(f) => (
          <AuthCard title="Не удалось открыть Ferrum">
            <p class="fe-auth__error" role="alert">
              {failureText(f())}
            </p>
            <button class="fe-btn fe-btn--primary fe-auth__wide" disabled={retrying()} onClick={retry}>
              Повторить
            </button>
          </AuthCard>
        )}
      </Match>
      <Match when={screen()?.kind === 'app'}>{props.children}</Match>
      {/* Вход не пересоздаётся при смене адреса: «Проверьте почту» и набранный адрес остаются. */}
      <Match when={screen()?.kind === 'login'}>
        <LoginScreen />
      </Match>
      <Match when={confirm()} keyed>
        {(c) => <ConfirmScreen token={c.token} next={c.next} onDone={signedIn} onNewLink={(next) => go(next ?? '#/')} />}
      </Match>
      <Match when={naming()} keyed>
        {(user) => <NameScreen user={user} onDone={named} />}
      </Match>
    </Switch>
  );
}

/** Смена экрана заметна и без зрения: фокус уходит на поле, а если поля нет, на заголовок
 * (прежняя кнопка с фокусом исчезла, а новый заголовок иначе не объявляется). */
function AuthCard(props: { title: string; children: JSX.Element }) {
  let card!: HTMLElement;
  let title!: HTMLHeadingElement;
  onMount(() => {
    dismissSplash(true);
    (card.querySelector<HTMLElement>('input') ?? title).focus();
  });
  return (
    <div class="fe-auth">
      <main class="fe-auth__card" ref={card}>
        <div class="fe-files__brand">
          <span class="fe-panel-head__mark" aria-hidden="true" />
          <span class="t-strong">Ferrum</span>
        </div>
        <h1 class="fe-auth__title" tabindex="-1" ref={title}>
          {props.title}
        </h1>
        {props.children}
      </main>
    </div>
  );
}

const PROVIDERS = ['Войти через Google', 'Войти через GitHub'];

function LoginScreen() {
  const [email, setEmail] = createSignal('');
  const [sentTo, setSentTo] = createSignal<string | null>(null);
  const [sentAt, setSentAt] = createSignal(0);
  const [now, setNow] = createSignal(Date.now());
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  /** Только ошибка самого адреса: сбой сети или предел писем поле не портят. */
  const [invalid, setInvalid] = createSignal(false);
  const timer = setInterval(() => setNow(Date.now()), 1000);
  onCleanup(() => clearInterval(timer));

  const send = async (address: string) => {
    setBusy(true);
    setError(null);
    setInvalid(false);
    // Адрес на момент отправки: он мог смениться, пока экран открыт.
    const r = await requestLink(address, parseNext(location.hash));
    setBusy(false);
    if (!r.ok) {
      setError(failureText(r.failure));
      return;
    }
    setSentTo(address);
    setSentAt(Date.now());
    setNow(Date.now());
  };
  const left = () => resendLeft(sentAt(), now());

  return (
    <Show
      when={sentTo()}
      fallback={
        <AuthCard title="Вход в Ferrum">
          <form
            class="fe-auth__form"
            novalidate
            onSubmit={(e) => {
              e.preventDefault();
              const address = parseEmail(email());
              if (address) void send(address);
              else {
                setError('Проверьте адрес: например, ivan@example.com');
                setInvalid(true);
              }
            }}
          >
            <label class="fe-auth__label" for="fe-auth-email">
              Электронная почта
            </label>
            <div class="fe-field fe-field--text fe-auth__field">
              <input
                id="fe-auth-email"
                type="email"
                autocomplete="email"
                required
                value={email()}
                aria-invalid={invalid()}
                onInput={(e) => setEmail(e.currentTarget.value)}
              />
            </div>
            <Show when={error()}>
              <p class="fe-auth__error" role="alert">
                {error()}
              </p>
            </Show>
            <button class="fe-btn fe-btn--primary fe-auth__wide" type="submit" disabled={busy()}>
              Получить ссылку
            </button>
          </form>
          <div class="fe-auth__or" role="separator">
            или
          </div>
          <For each={PROVIDERS}>
            {(name) => (
              // `aria-disabled`, а не `disabled`: подсказка должна открываться и с клавиатуры.
              <button
                class="fe-btn fe-btn--secondary fe-auth__wide fe-auth__soon"
                type="button"
                aria-disabled="true"
                onClick={(e) => e.preventDefault()}
              >
                {name}
                <span class="fe-tooltip fe-auth__tip" role="tooltip">
                  Скоро
                </span>
              </button>
            )}
          </For>
        </AuthCard>
      }
    >
      {(address) => (
        <AuthCard title="Проверьте почту">
          <p class="fe-auth__text">
            Мы отправили ссылку для входа на <b>{address()}</b>. Она действует 15 минут.
          </p>
          <Show when={error()}>
            <p class="fe-auth__error" role="alert">
              {error()}
            </p>
          </Show>
          <button class="fe-btn fe-btn--secondary fe-auth__wide" disabled={left() > 0 || busy()} onClick={() => void send(address())}>
            {left() > 0 ? `Отправить ещё раз через ${left()} с` : 'Отправить ещё раз'}
          </button>
          <button
            class="fe-btn fe-auth__wide"
            onClick={() => {
              setError(null);
              setSentTo(null);
            }}
          >
            Другой адрес
          </button>
        </AuthCard>
      )}
    </Show>
  );
}

/** Ссылка из письма: кнопка вместо входа по GET, почтовые сканеры открывают ссылки и сожгли бы
 * одноразовый токен. Токен тратится только по нажатию. Ссылка точно мертва только при отказе
 * `INVALID_TOKEN` (или если токена нет). При любом другом сбое «Войти» остаётся повтором, а токен в
 * адресе не трогается. Но при сбое сервера токен мог быть уже потрачен (Better Auth снимает его до
 * обращения к базе): тогда рядом и «Запросить новую ссылку». */
function ConfirmScreen(props: {
  token: string | null;
  next: string | null;
  onDone: (next: string | null) => void;
  onNewLink: (next: string | null) => void;
}) {
  const [busy, setBusy] = createSignal(false);
  const [dead, setDead] = createSignal(!props.token);
  const [serverDown, setServerDown] = createSignal(false);
  const [error, setError] = createSignal<string | null>(props.token ? null : failureText('INVALID_TOKEN'));
  const enter = async () => {
    setBusy(true);
    setError(null);
    setServerDown(false);
    const r = await confirmLink(props.token!);
    if (r.ok) {
      props.onDone(props.next);
      return;
    }
    setBusy(false);
    setError(failureText(r.failure));
    if (r.failure === 'INVALID_TOKEN') setDead(true);
    if (r.failure === 'UNAVAILABLE') setServerDown(true);
  };
  return (
    <AuthCard title="Вход в Ferrum">
      <Show when={error()}>
        <p class="fe-auth__error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={!dead()}>
        <Show when={!error()}>
          <p class="fe-auth__text">Нажмите кнопку, чтобы завершить вход.</p>
        </Show>
        <button class="fe-btn fe-btn--primary fe-auth__wide" disabled={busy()} onClick={() => void enter()}>
          Войти
        </button>
      </Show>
      <Show when={dead() || serverDown()}>
        <button
          class={`fe-btn ${dead() ? 'fe-btn--primary' : 'fe-btn--secondary'} fe-auth__wide`}
          onClick={() => props.onNewLink(parseNext(props.next))}
        >
          Запросить новую ссылку
        </button>
      </Show>
    </AuthCard>
  );
}

/** Экран первого входа: имя видят другие участники. */
function NameScreen(props: { user: SessionUser; onDone: (user: SessionUser) => void }) {
  const [name, setName] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  return (
    <AuthCard title="Как вас зовут?">
      <p class="fe-auth__text">Это имя увидят те, с кем вы работаете. Вы вошли как {props.user.email}.</p>
      <form
        class="fe-auth__form"
        novalidate
        onSubmit={(e) => {
          e.preventDefault();
          if (!name().trim()) {
            setError('Введите имя');
            return;
          }
          setBusy(true);
          setError(null);
          void saveName(name()).then((r) => {
            setBusy(false);
            if (r.ok) props.onDone(r.value.user);
            else setError(failureText(r.failure));
          });
        }}
      >
        <label class="fe-auth__label" for="fe-auth-name">
          Имя
        </label>
        <div class="fe-field fe-field--text fe-auth__field">
          <input
            id="fe-auth-name"
            autocomplete="name"
            maxlength={MAX_NAME_CHARS}
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
        </div>
        <Show when={error()}>
          <p class="fe-auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <button class="fe-btn fe-btn--primary fe-auth__wide" type="submit" disabled={busy()}>
          Продолжить
        </button>
      </form>
    </AuthCard>
  );
}
