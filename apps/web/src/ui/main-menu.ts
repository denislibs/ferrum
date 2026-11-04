// Главное меню (спека подсистемы 2, раздел 4): пункты из дерева ядра
// (`MainMenu`) с названиями из таблицы команд и поиск по командам меню — тот
// же нечёткий поиск, что в «Быстрых действиях» (`fuzzy.ts`, `palette.ts`), без
// учёта регистра, «ё» и раскладки (запрос «щквук» находит «order»: буквы
// переводятся ЙЦУКЕН ↔ QWERTY).
// Основной поток ничего не вычисляет о документе: доступность и галочки —
// из состояния команд ядра, здесь — только названия и отбор по строке.

import type { KeymapEntry, MenuItemDto } from '../wasm/ferrum_wasm';
import type { MenuItem } from './menu';
import { layoutVariants, normalize } from './fuzzy';
import { rankCommands } from './palette';

export { layoutVariants, normalize };

/** Сколько результатов поиска показывать (как у «Быстрых действий»). */
export const MAX_RESULTS = 50;

type Entries = ReadonlyMap<string, KeymapEntry>;

export function entriesByKey(commands: readonly KeymapEntry[]): Map<string, KeymapEntry> {
  return new Map(commands.map((c) => [c.key, c]));
}

/** Убирает разделители в начале, в конце и подряд (после пропуска пунктов). */
export function tidy(items: MenuItem[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const it of items) {
    if ('separator' in it && (out.length === 0 || 'separator' in out[out.length - 1])) continue;
    out.push(it);
  }
  while (out.length && 'separator' in out[out.length - 1]) out.pop();
  return out;
}

/** Пункты меню из дерева ядра. `hidden` — команды, которых на этой платформе
 * нет (файл на диске без File System Access): пункты пропускаются, пустые
 * подменю тоже. */
export function buildMainMenu(tree: readonly MenuItemDto[], entries: Entries, hidden: ReadonlySet<string> = new Set()): MenuItem[] {
  const out: MenuItem[] = [];
  for (const n of tree) {
    if (n.type === 'separator') out.push({ separator: true });
    else if (n.type === 'submenu') {
      const items = buildMainMenu(n.items, entries, hidden);
      if (items.length) out.push({ label: n.title, submenu: items });
    } else if (n.type === 'command' && !hidden.has(n.key)) {
      const e = entries.get(n.key);
      if (e) out.push({ label: e.name, command: e.key });
    }
  }
  return tidy(out);
}

/** Ключи команд дерева по порядку, без повторов. */
function treeKeys(tree: readonly MenuItemDto[], out: string[] = []): string[] {
  for (const n of tree) {
    if (n.type === 'command' && !out.includes(n.key)) out.push(n.key);
    else if (n.type === 'submenu') treeKeys(n.items, out);
  }
  return out;
}

/** Поиск по командам меню, как в «Быстрых действиях»: нечётко по названию и
 * синонимам в любой раскладке, лучшие первыми, при равном весе — в порядке
 * меню; не больше `MAX_RESULTS`. */
export function searchMenu(
  tree: readonly MenuItemDto[],
  entries: Entries,
  query: string,
  hidden: ReadonlySet<string> = new Set(),
): MenuItem[] {
  const list: KeymapEntry[] = [];
  for (const key of treeKeys(tree)) {
    const e = entries.get(key);
    if (e && !hidden.has(key)) list.push(e);
  }
  return rankCommands(list, query, [], MAX_RESULTS).map((e) => ({ label: e.name, command: e.key }));
}
