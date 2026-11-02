// Главное меню (спека подсистемы 2, раздел 4): пункты из дерева ядра
// (`MainMenu`) с названиями из таблицы команд и поиск по командам меню —
// подстрока названия или синонима, без учёта регистра, «ё» и раскладки
// (запрос «щквук» находит «order»: буквы переводятся ЙЦУКЕН ↔ QWERTY).
// Основной поток ничего не вычисляет о документе: доступность и галочки —
// из состояния команд ядра, здесь — только названия и отбор по строке.

import type { KeymapEntry, MenuItemDto } from '../wasm/ferrum_wasm';
import type { MenuItem } from './menu';

/** Сколько результатов поиска показывать (как у «Быстрых действий»). */
export const MAX_RESULTS = 50;

const EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU = 'йцукенгшщзхъфывапролджэячсмитьбюё';

const toRu = new Map([...EN].map((c, i) => [c, RU[i]]));
const toEn = new Map([...RU].map((c, i) => [c, EN[i]]));

/** Строка для сравнения: нижний регистр, «ё» как «е», без пробелов по краям. */
export function normalize(s: string): string {
  return s.trim().toLowerCase().replaceAll('ё', 'е');
}

/** Запрос в обеих раскладках: как набран, ЙЦУКЕН → QWERTY и обратно. */
export function layoutVariants(query: string): string[] {
  const q = query.trim().toLowerCase();
  const swap = (m: Map<string, string>) => [...q].map((c) => m.get(c) ?? c).join('');
  return [...new Set([q, swap(toRu), swap(toEn)].map(normalize))].filter(Boolean);
}

type Entries = ReadonlyMap<string, KeymapEntry>;

export function entriesByKey(commands: readonly KeymapEntry[]): Map<string, KeymapEntry> {
  return new Map(commands.map((c) => [c.key, c]));
}

/** Убирает разделители в начале, в конце и подряд (после пропуска пунктов). */
function tidy(items: MenuItem[]): MenuItem[] {
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
    } else if (!hidden.has(n.key)) {
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

/** Поиск по командам меню: подстрока названия или синонима в любой раскладке.
 * Сначала команды, чьё название начинается с запроса, затем остальные — в
 * порядке меню; не больше `MAX_RESULTS`. */
export function searchMenu(tree: readonly MenuItemDto[], entries: Entries, query: string, hidden: ReadonlySet<string> = new Set()): MenuItem[] {
  const variants = layoutVariants(query);
  if (!variants.length) return [];
  const first: MenuItem[] = [];
  const rest: MenuItem[] = [];
  for (const key of treeKeys(tree)) {
    const e = entries.get(key);
    if (!e || hidden.has(key)) continue;
    const name = normalize(e.name);
    const words = e.keywords.map(normalize);
    const item: MenuItem = { label: e.name, command: e.key };
    if (variants.some((v) => name.startsWith(v))) first.push(item);
    else if (variants.some((v) => name.includes(v) || words.some((w) => w.includes(v)))) rest.push(item);
  }
  return [...first, ...rest].slice(0, MAX_RESULTS);
}
