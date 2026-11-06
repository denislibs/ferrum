// Аватары участников в шапке (спека подсистемы 4, раздел 9): по человеку один аватар, сколько бы вкладок у него ни было; больше
// пяти — счётчик со списком. Щелчок — следовать за участником, ещё раз — перестать. Себя в полосе нет: свой аватар — в меню
// аккаунта, свои другие вкладки видны курсором (поведение). Список приходит из `sync.worker` не чаще четырёх раз в секунду.

import { createMemo, createSignal, For, Show } from 'solid-js';
import type { PresenceBarProps } from '../extensions';
import { Menu, type MenuItem } from '../menu';

/** Аватаров в полосе, остальные — в счётчике. */
export const AVATARS_SHOWN = 5;

interface Person {
  user: string;
  name: string;
  color: string;
  /** Участник человека, за которым следовать: новейшее соединение. */
  participant: number;
  /** Все участники человека: за кем из них следуем. */
  ids: number[];
  elsewhere: boolean;
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const letters = parts.length > 1 ? parts[0][0] + parts[1][0] : (parts[0] ?? '?').slice(0, 1);
  return letters.toUpperCase();
}

export function PresenceBar(props: PresenceBarProps) {
  const [menu, setMenu] = createSignal<HTMLElement | null>(null);
  const people = createMemo<Person[]>(() => {
    const by = new Map<string, Person>();
    for (const p of props.participants()) {
      if (p.user === props.self) continue;
      const was = by.get(p.user);
      const elsewhere = p.page !== null && p.page !== props.currentPage();
      if (!was) {
        by.set(p.user, { user: p.user, name: p.name, color: p.color, participant: p.id, ids: [p.id], elsewhere });
      } else {
        was.ids.push(p.id);
        if (p.id > was.participant) was.participant = p.id;
        was.elsewhere &&= elsewhere;
      }
    }
    return [...by.values()];
  });
  const isFollowed = (p: Person) => {
    const f = props.following();
    return f !== null && p.ids.includes(f);
  };
  const toggle = (p: Person) => props.follow(isFollowed(p) ? null : p.participant);
  const title = (p: Person) =>
    `${p.name}${p.elsewhere ? ' — на другой странице' : ''}. ${isFollowed(p) ? 'Перестать следовать' : 'Следовать'}`;
  const items = (): MenuItem[] =>
    people().map((p) => ({
      label: `${p.name}${p.elsewhere ? ' (другая страница)' : ''}`,
      checked: isFollowed(p),
      onSelect: () => toggle(p),
    }));
  return (
    <Show when={people().length > 0}>
      <div class="fe-presence" role="group" aria-label="Участники">
        <For each={people().slice(0, AVATARS_SHOWN)}>
          {(p) => (
            <button
              class="fe-avatar"
              classList={{ 'fe-avatar--followed': isFollowed(p), 'fe-avatar--away': p.elsewhere }}
              style={{ '--avatar': p.color }}
              aria-pressed={isFollowed(p)}
              aria-label={title(p)}
              data-tip={title(p)}
              data-user={p.user}
              onClick={() => toggle(p)}
            >
              {initials(p.name)}
            </button>
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
