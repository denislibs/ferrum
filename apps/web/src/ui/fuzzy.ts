// Нечёткий поиск «Быстрых действий» и главного меню (спека подсистемы 2,
// раздел 7): буквы запроса должны встретиться в тексте по порядку
// (подпоследовательность), вес растёт за начало слова, буквы подряд и начало
// текста и падает за пропуски. Регистр и «ё» не важны, раскладка тоже: запрос
// пробуется как набран и переведённым ЙЦУКЕН ↔ QWERTY («щквук» находит «order»).
// Пробелы запроса не учитываются. Тот же счёт — у поиска слоёв в ядре
// (`crates/ferrum-editor/src/find.rs`): веса и правила совпадают.

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

/** Сколько первых символов текста участвует в поиске (как `FIND_NAME_CHARS` ядра). */
export const MAX_TEXT_CHARS = 40;

const MATCH = 1;
const WORD_START = 8;
const CONSECUTIVE = 5;
const PREFIX = 6;
const MAX_GAP = 3;
const EXACT = 10;

/** Символы для сравнения: нижний регистр, «ё» как «е», первые `MAX_TEXT_CHARS`. */
export function fold(s: string): string[] {
  const head = [...s].slice(0, MAX_TEXT_CHARS).join('');
  return [...head.toLowerCase().replaceAll('ё', 'е')].slice(0, MAX_TEXT_CHARS);
}

/** Варианты запроса для `scoreFolded`: обе раскладки, без пробелов; пусто — нечего искать. */
export function queryVariants(query: string): string[][] {
  const out: string[][] = [];
  for (const v of layoutVariants(query)) {
    const q = [...v.replace(/\s+/gu, '')];
    if (q.length && !out.some((o) => o.join('') === q.join(''))) out.push(q);
  }
  return out;
}

const ALNUM = /[\p{L}\p{N}]/u;

/** Начало слова: первый символ или после символа, который не буква и не цифра. */
function isStart(t: string[], j: number): boolean {
  return j === 0 || !ALNUM.test(t[j - 1]);
}

function scorePositions(t: string[], pos: number[]): number {
  let s = 0;
  pos.forEach((j, i) => {
    s += MATCH;
    if (isStart(t, j)) s += WORD_START;
    if (i > 0) {
      const gap = j - pos[i - 1] - 1;
      s += gap === 0 ? CONSECUTIVE : -Math.min(gap, MAX_GAP);
    }
  });
  if (pos[0] === 0) s += PREFIX;
  else s -= Math.min(pos[0], MAX_GAP);
  // Совпали все буквы текста (пробелы запроса не учитываются — и текста тоже).
  if (pos.length === t.filter((c) => !/\s/u.test(c)).length) s += EXACT;
  return s;
}

/** Буквы запроса по порядку: каждая — первое вхождение после прошлой; с
 * `preferStart` — сначала вхождение в начале слова, если оно есть. */
function align(t: string[], q: string[], preferStart: boolean): number[] | null {
  const pos: number[] = [];
  let from = 0;
  for (const qc of q) {
    let j = -1;
    if (preferStart) {
      for (let k = from; k < t.length; k++) {
        if (t[k] === qc && isStart(t, k)) {
          j = k;
          break;
        }
      }
    }
    if (j < 0) j = t.indexOf(qc, from);
    if (j < 0) return null;
    pos.push(j);
    from = j + 1;
  }
  return pos;
}

/** Запрос подстрокой: вхождение в начале слова, иначе первое. */
function substring(t: string[], q: string[]): number[] | null {
  let best = -1;
  for (let i = 0; i + q.length <= t.length; i++) {
    let ok = true;
    for (let k = 0; k < q.length && ok; k++) ok = t[i + k] === q[k];
    if (!ok) continue;
    if (isStart(t, i)) {
      best = i;
      break;
    }
    if (best < 0) best = i;
  }
  return best < 0 ? null : q.map((_, k) => best + k);
}

/** Вес совпадения одного варианта запроса со сложенным текстом; `null` — не совпало. */
export function scoreFolded(t: string[], q: string[]): number | null {
  if (!q.length) return null;
  const greedy = align(t, q, false);
  if (!greedy) return null;
  let best = scorePositions(t, greedy);
  for (const pos of [align(t, q, true), substring(t, q)]) {
    if (pos) best = Math.max(best, scorePositions(t, pos));
  }
  return best;
}

/** Лучший вес по вариантам запроса (`queryVariants`); `null` — не совпало ни одним. */
export function scoreVariants(text: string, variants: readonly string[][]): number | null {
  const t = fold(text);
  let best: number | null = null;
  for (const q of variants) {
    const s = scoreFolded(t, q);
    if (s !== null && (best === null || s > best)) best = s;
  }
  return best;
}

/** Вес совпадения запроса с текстом в любой раскладке; `null` — не совпало. */
export function fuzzyScore(text: string, query: string): number | null {
  return scoreVariants(text, queryVariants(query));
}
