// Аватары участников в шапке (спека подсистемы 4, раздел 9): по человеку один аватар, сколько бы вкладок у него ни было; больше
// пяти — счётчик со списком. Щелчок — следовать за участником, ещё раз — перестать. Себя в полосе нет: свой аватар — в меню
// аккаунта, свои другие вкладки видны курсором (поведение). Список приходит из `sync.worker` не чаще четырёх раз в секунду.

import { createEffect, createMemo, createSignal, For, Show } from 'solid-js';
import type { PresenceBarProps } from '../extensions';
import { Menu, type MenuItem } from '../menu';
import { avatarLabel, groupPeople, initials, type Person } from './presence-people';

/** Аватаров в полосе, остальные — в счётчике. */
export const AVATARS_SHOWN = 5;

export function PresenceBar(props: PresenceBarProps) {
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const people = createMemo<Person[]>(() => groupPeople(props.participants(), props.self, props.currentPage()));
  const byUser = createMemo(() => new Map(people().map((p) => [p.user, p])));
  // Кнопки — по человеку, а не по объекту списка: список приходит заново при любом изменении (вход, уход, смена страницы), а
  // кнопка с фокусом и подсказкой остаётся той же.
  const shown = createMemo(() =>
    people()
      .slice(0, AVATARS_SHOWN)
      .map((p) => p.user),
  );
  // Счётчика больше нет — нет и его меню: иначе оно висело бы у пропавшего якоря и открылось бы само, когда людей снова станет больше.
  createEffect(() => {
    if (people().length <= AVATARS_SHOWN) setMenu(null);
  });
  const isFollowed = (p: Person) => {
    const f = props.following();
    return f !== null && p.ids.includes(f);
  };
  const toggle = (p: Person) => props.follow(isFollowed(p) ? null : p.participant);
  const items = (): MenuItem[] =>
    people().map((p) => ({
      label: `${p.name}${p.elsewhere ? ' (другая страница)' : ''}`,
      checked: isFollowed(p),
      onSelect: () => toggle(p),
    }));
  return (
    <Show when={people().length > 0}>
      <div class="fe-presence" role="group" aria-label="Участники">
        <For each={shown()}>
          {(user) => (
            <Show when={byUser().get(user)}>
              {(p) => (
                <button
                  class="fe-avatar"
                  classList={{ 'fe-avatar--followed': isFollowed(p()), 'fe-avatar--away': p().elsewhere }}
                  style={{ '--avatar': p().color }}
                  aria-pressed={isFollowed(p())}
                  aria-label={avatarLabel(p())}
                  data-tip={avatarLabel(p())}
                  data-user={user}
                  onClick={() => toggle(p())}
                >
                  {initials(p().name)}
                </button>
              )}
            </Show>
          )}
        </For>
        <Show when={people().length > AVATARS_SHOWN}>
          <button
            class="fe-avatar fe-avatar--more"
            aria-haspopup="menu"
            aria-expanded={!!menu()}
            aria-label={`Все участники: ${people().length}`}
            data-tip="Все участники"
            onClick={(e) => setMenu(e.currentTarget)}
          >
            +{people().length - AVATARS_SHOWN}
          </button>
        </Show>
        <Show when={menu()}>{(a) => <Menu label="Участники" items={items()} anchor={a()} onClose={() => setMenu(null)} />}</Show>
      </div>
    </Show>
  );
}
