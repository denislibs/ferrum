// Математика поля числа (NumberField): выражения (`120+16`, `50%`, `*2`),
// шаг перетаскивания подписи (Shift ×10, Alt ×0.1), клавиши поля, формат.

/** Что ввёл пользователь: новое значение или прибавка к значению каждого узла. */
export type FieldInput = { kind: 'set'; value: number } | { kind: 'add'; delta: number };

type Tok = { t: 'num'; v: number } | { t: 'op'; v: string };

/** Токены выражения; запятая — тоже десятичный разделитель (русская раскладка). */
function tokenize(text: string): Tok[] | null {
  const out: Tok[] = [];
  const s = text.replace(/\s+/g, '').replace(/,/g, '.').replace(/[×х]/g, '*').replace(/[−–]/g, '-');
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    const m = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(s.slice(i));
    if (m) {
      out.push({ t: 'num', v: Number(m[0]) });
      i += m[0].length;
    } else if ('+-*/()%'.includes(c)) {
      out.push({ t: 'op', v: c });
      i++;
    } else {
      return null;
    }
  }
  return out;
}

/** Разбор с приоритетами: `+ -` < `* /` < унарный минус < `%` < скобки. Процент —
 * доля от `percentOf` (у ширины — от текущей ширины, у прозрачности — само число). */
function parse(tokens: Tok[], percentOf: number): number | null {
  let i = 0;
  const peek = () => tokens[i];
  const op = (v: string) => peek()?.t === 'op' && peek().v === v;
  function primary(): number | null {
    if (op('(')) {
      i++;
      const v = sum();
      if (v === null || !op(')')) return null;
      i++;
      return v;
    }
    if (op('-')) {
      i++;
      const v = postfix();
      return v === null ? null : -v;
    }
    if (op('+')) {
      i++;
      return postfix();
    }
    const t = peek();
    if (t?.t !== 'num') return null;
    i++;
    return t.v;
  }
  function postfix(): number | null {
    let v = primary();
    while (v !== null && op('%')) {
      i++;
      v = (v / 100) * percentOf;
    }
    return v;
  }
  function product(): number | null {
    let v = postfix();
    while (v !== null && (op('*') || op('/'))) {
      const o = tokens[i++].v;
      const r = postfix();
      if (r === null) return null;
      v = o === '*' ? v * r : v / r;
    }
    return v;
  }
  function sum(): number | null {
    let v = product();
    while (v !== null && (op('+') || op('-'))) {
      const o = tokens[i++].v;
      const r = product();
      if (r === null) return null;
      v = o === '+' ? v + r : v - r;
    }
    return v;
  }
  const v = sum();
  return v !== null && i === tokens.length && Number.isFinite(v) ? v : null;
}

export interface ParseOptions {
  /** Текущее значение (у «Смешанных» — первого узла). */
  current: number;
  /** Значения у выделенных разные: «+10» прибавит 10 к каждому. */
  mixed?: boolean;
  /** От чего считать «%» (по умолчанию от текущего значения). */
  percentOf?: number;
}

/** Ввод поля. Выражение, начатое с `+ * /`, продолжает текущее значение (`*2` —
 * вдвое); у «Смешанных» `+N` и `-N` с пробелом нет — это прибавка к каждому узлу.
 * `null` — не число. */
export function parseFieldInput(text: string, opts: ParseOptions): FieldInput | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const tokens = tokenize(trimmed);
  if (!tokens || tokens.length === 0) return null;
  const percentOf = opts.percentOf ?? opts.current;
  const first = tokens[0];
  const continues = first.t === 'op' && (first.v === '*' || first.v === '/' || first.v === '+');
  if (opts.mixed && first.t === 'op' && (first.v === '+' || first.v === '-')) {
    const delta = parse(tokens, percentOf);
    return delta === null ? null : { kind: 'add', delta };
  }
  const value = parse(continues ? [{ t: 'num', v: opts.current }, ...tokens] : tokens, percentOf);
  return value === null ? null : { kind: 'set', value };
}

/** Модификаторы перетаскивания подписи. */
export interface ScrubMods {
  shift: boolean;
  alt: boolean;
}

/** Множитель шага: Shift — ×10, Alt — ×0.1 (дизайн-система, NumberField). */
export function scrubFactor(mods: ScrubMods): number {
  if (mods.shift) return 10;
  if (mods.alt) return 0.1;
  return 1;
}

/** Значение при перетаскивании подписи на `dx` пикселей от `start`: шаг за
 * пиксель, округление до точности шага (без хвостов вида 0.30000000000000004). */
export function scrubValue(start: number, dx: number, step: number, mods: ScrubMods): number {
  const k = step * scrubFactor(mods);
  const v = start + Math.round(dx) * k;
  const digits = Math.max(0, Math.ceil(-Math.log10(k)));
  return Number(v.toFixed(Math.min(digits, 6)));
}

/** Прибавка стрелкой в поле: 1, с Shift — 10. */
export function arrowStep(key: string, shift: boolean): number {
  const d = shift ? 10 : 1;
  if (key === 'ArrowUp') return d;
  if (key === 'ArrowDown') return -d;
  return 0;
}

/** Что делать полю по клавише: Enter применяет, Esc откатывает. */
export function fieldKey(key: string): 'commit' | 'revert' | null {
  if (key === 'Enter' || key === 'NumpadEnter') return 'commit';
  if (key === 'Escape') return 'revert';
  return null;
}

/** Число для поля: до `decimals` знаков, без лишних нулей, точка как разделитель. */
export function formatNumber(v: number, decimals = 2): string {
  if (!Number.isFinite(v)) return '';
  const r = Number(v.toFixed(decimals));
  return String(Object.is(r, -0) ? 0 : r);
}

export function clamp(v: number, min = -Infinity, max = Infinity): number {
  return Math.min(max, Math.max(min, v));
}
