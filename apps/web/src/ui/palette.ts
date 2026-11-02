// «Быстрые действия» (спека подсистемы 2, раздел 7): что показать в списке
// палитры. Источники — недавние команды, команды таблицы, слои текущей страницы
// (их ищет worker: `FindLayers` → `LayersFound`) и страницы; один список с
// заголовками, не больше `PALETTE_ROWS` строк. Команды ищутся здесь, в основном
// потоке (их ≈ 90): нечётко по названию и синонимам (`fuzzy.ts`), с весом за
// недавность. Недавние — последние `RECENT_MAX` выполненных команд, хранятся в
// настройках строкой ключей через пробел.

import type { FoundLayerDto, KeymapEntry, NodeType, PageInfo } from '../wasm/ferrum_wasm';
import { queryVariants, scoreVariants } from './fuzzy';

/** Строк в списке, не считая заголовков (как у поиска главного меню). */
export const PALETTE_ROWS = 50;
/** Недавних команд. */
export const RECENT_MAX = 8;
/** Сколько слоёв просить у worker'а (`FindLayers.limit`). */
export const LAYER_LIMIT = 50;
/** Предел запроса поиска слоёв в байтах UTF-8 (как у ядра). */
export const MAX_QUERY_BYTES = 256;
/** Команд и страниц при непустом запросе: остальное место — слоям. */
export const COMMANDS_MAX = 20;
export const PAGES_MAX = 10;
/** Синоним весит меньше названия; недавняя команда — больше остальных. */
const KEYWORD_PENALTY = 4;
const RECENT_BONUS = 3;

/** Команды, которых нет в палитре: удержание клавиши, Esc, сдвиги и сама палитра. */
const EXCLUDED = new Set(['tool.hand_hold', 'edit.escape', 'ui.quick_actions']);

export function inPalette(e: KeymapEntry): boolean {
  return e.group !== 'nudge' && !EXCLUDED.has(e.key);
}

export type PaletteRow =
  | { kind: 'heading'; title: string }
  | { kind: 'command'; key: string }
  | { kind: 'layer'; id: string; name: string; type: NodeType }
  | { kind: 'page'; id: string; name: string };

const KEY = /^[a-z]+\.[a-z0-9_]+$/;

/** Недавние из настроек (не доверенные): ключи команд через пробел, без повторов. */
export function parseRecent(s: string): string[] {
  const out: string[] = [];
  for (const k of s.split(' ')) {
    if (k.length <= 64 && KEY.test(k) && !out.includes(k)) out.push(k);
    if (out.length === RECENT_MAX) break;
  }
  return out;
}

/** Выполненная команда — в начало недавних (повтор переезжает), не больше `RECENT_MAX`. */
export function pushRecent(recent: readonly string[], key: string): string[] {
  if (!KEY.test(key)) return [...recent];
  return [key, ...recent.filter((k) => k !== key)].slice(0, RECENT_MAX);
}

/** Сочетание → ключ команды (первая в таблице, если занято двумя): для отметки
 * использованных сочетаний панелью сочетаний. */
export function bindingsToCommands(commands: readonly KeymapEntry[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const c of commands) {
    for (const b of c.bindings) if (!out.has(b)) out.set(b, c.key);
  }
  return out;
}

/** Вес команды по запросу: лучшее из названия и синонимов (синоним — с вычетом),
 * плюс недавность; `null` — не совпала. */
export function scoreCommand(e: KeymapEntry, variants: readonly string[][], recent: readonly string[] = []): number | null {
  let best = scoreVariants(e.name, variants);
  for (const k of e.keywords) {
    const s = scoreVariants(k, variants);
    if (s !== null && (best === null || s - KEYWORD_PENALTY > best)) best = s - KEYWORD_PENALTY;
  }
  if (best === null) return null;
  const r = recent.indexOf(e.key);
  return r < 0 ? best : best + (RECENT_MAX - r) * RECENT_BONUS;
}

/** Совпавшие команды, лучшие первыми, при равном весе — в порядке `entries`. */
export function rankCommands(entries: readonly KeymapEntry[], query: string, recent: readonly string[] = [], limit = PALETTE_ROWS): KeymapEntry[] {
  const variants = queryVariants(query);
  if (!variants.length) return [];
  const hits: { e: KeymapEntry; s: number; i: number }[] = [];
  entries.forEach((e, i) => {
    const s = scoreCommand(e, variants, recent);
    if (s !== null) hits.push({ e, s, i });
  });
  hits.sort((a, b) => b.s - a.s || a.i - b.i);
  return hits.slice(0, limit).map((h) => h.e);
}

/** Запрос, обрезанный до `MAX_QUERY_BYTES` байт UTF-8 по границе символа. */
export function clipQuery(q: string): string {
  const enc = new TextEncoder();
  if (enc.encode(q).length <= MAX_QUERY_BYTES) return q;
  let out = '';
  let bytes = 0;
  for (const c of q) {
    const n = enc.encode(c).length;
    if (bytes + n > MAX_QUERY_BYTES) break;
    out += c;
    bytes += n;
  }
  return out;
}

export interface PaletteSources {
  query: string;
  /** Таблица команд (`KeymapEntry`) в порядке ядра. */
  commands: readonly KeymapEntry[];
  /** Команды, которых нет на этой платформе (файл на диске без File System Access). */
  hidden: ReadonlySet<string>;
  recent: readonly string[];
  /** Ответ worker'а на этот запрос (на прежний — не передавать). */
  layers: readonly FoundLayerDto[];
  pages: readonly PageInfo[];
}

/** Строки палитры. Пустой запрос — недавние и все команды; иначе — команды (не
 * больше `COMMANDS_MAX`), слои и страницы (не больше `PAGES_MAX`) с заголовками
 * непустых разделов; строк с действиями — не больше `PALETTE_ROWS`. */
export function paletteRows(src: PaletteSources): PaletteRow[] {
  const usable = src.commands.filter((e) => inPalette(e) && !src.hidden.has(e.key));
  const out: PaletteRow[] = [];
  let left = PALETTE_ROWS;
  const section = (title: string, rows: PaletteRow[]) => {
    const take = rows.slice(0, left);
    if (!take.length) return;
    out.push({ kind: 'heading', title }, ...take);
    left -= take.length;
  };
  if (!queryVariants(src.query).length) {
    const byKey = new Map(usable.map((e) => [e.key, e]));
    const recent = src.recent.filter((k) => byKey.has(k));
    section('Недавние', recent.map((key) => ({ kind: 'command', key })));
    section('Команды', usable.filter((e) => !recent.includes(e.key)).map((e) => ({ kind: 'command', key: e.key })));
    return out;
  }
  const commands = rankCommands(usable, src.query, src.recent, COMMANDS_MAX);
  const variants = queryVariants(src.query);
  const pages = src.pages
    .map((p, i) => ({ p, i, s: scoreVariants(p.name, variants) }))
    .filter((x): x is { p: PageInfo; i: number; s: number } => x.s !== null)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .slice(0, PAGES_MAX)
    .map((x): PaletteRow => ({ kind: 'page', id: x.p.id, name: x.p.name }));
  const layerRoom = PALETTE_ROWS - Math.min(commands.length, PALETTE_ROWS) - pages.length;
  section('Команды', commands.map((e) => ({ kind: 'command', key: e.key })));
  section(
    'Слои',
    src.layers.slice(0, Math.max(0, layerRoom)).map((l) => ({ kind: 'layer', id: l.id, name: l.name, type: l.kind })),
  );
  section('Страницы', pages);
  return out;
}
