// Шрифты на стороне TypeScript (спека подсистемы 3, раздел 4): разбор манифеста
// вшитых шрифтов, адрес файла вшитого шрифта и список системных шрифтов Chrome
// (`queryLocalFonts`) в виде команды `fontsAvailable`. Чистая логика — для
// модульных тестов; данные не доверенные и ограничены как в ядре.

import type { FontChoice, FontFamilyDto } from '../wasm/ferrum_wasm';
import { MAX_FAMILY_NAME, MAX_FONT_FACES, MAX_FONT_FAMILIES } from './limits';

const utf8 = new TextEncoder();

function isName(v: unknown): v is string {
  return typeof v === 'string' && v.length > 0 && v.length <= MAX_FAMILY_NAME && utf8.encode(v).length <= MAX_FAMILY_NAME;
}

/** Путь файла вшитого шрифта: относительный, из латиницы, цифр, `-_.` и `/`, без `..`. */
export function isFontPath(v: unknown): v is string {
  return typeof v === 'string' && v.length <= 256 && /^[A-Za-z0-9_-]+(\/[A-Za-z0-9_.-]+)*\.(ttf|otf)$/.test(v) && !v.includes('..');
}

/** Семейства манифеста (`public/fonts/manifest.json`), пригодные для ядра;
 * неверные записи пропускаются. */
export function manifestFamilies(json: unknown): FontFamilyDto[] {
  const list = (json as { families?: unknown })?.families;
  if (!Array.isArray(list)) return [];
  const out: FontFamilyDto[] = [];
  for (const f of list.slice(0, MAX_FONT_FAMILIES)) {
    if (!isName(f?.family) || !Array.isArray(f.faces)) continue;
    const faces = (f.faces as unknown[])
      .slice(0, MAX_FONT_FACES)
      .filter((x): x is Record<string, unknown> => typeof x === 'object' && x !== null)
      .filter((x) => isName(x.style) && isFontPath(x.file))
      .map((x) => ({
        style: x.style as string,
        weight: typeof x.weight === 'number' ? x.weight : undefined,
        italic: typeof x.italic === 'boolean' ? x.italic : undefined,
        stretch: typeof x.stretch === 'number' ? x.stretch : undefined,
        file: x.file as string,
        index: typeof x.index === 'number' ? x.index : undefined,
      }));
    if (faces.length) out.push({ family: f.family, faces });
  }
  return out;
}

/** Метаданные системного шрифта Chrome (`FontData` без байтов). */
export interface LocalFont {
  family: string;
  style: string;
  postscriptName: string;
}

/** Системные шрифты Chrome в семейства для `fontsAvailable`: файл начертания —
 * postscript-имя (по нему основной поток найдёт `FontData` за байтами). Вес и
 * курсив Chrome не сообщает — их угадывает ядро по имени начертания. */
export function localFamilies(fonts: readonly LocalFont[]): FontFamilyDto[] {
  const byFamily = new Map<string, FontFamilyDto>();
  for (const f of fonts) {
    if (!isName(f.family) || !isName(f.style) || !isName(f.postscriptName)) continue;
    let fam = byFamily.get(f.family);
    if (!fam) {
      if (byFamily.size >= MAX_FONT_FAMILIES) continue;
      fam = { family: f.family, faces: [] };
      byFamily.set(f.family, fam);
    }
    if (fam.faces.length >= MAX_FONT_FACES || fam.faces.some((x) => x.style === f.style)) continue;
    fam.faces.push({ style: f.style, file: f.postscriptName });
  }
  return [...byFamily.values()];
}

/** Список системных шрифтов десктопа (ответ `fonts_list`) в пределах ядра: файл
 * начертания — номер файла в списке Rust. Rust уже ограничивает список; здесь —
 * вторая граница: неверное семейство или начертание пропускается, лишнее сверх
 * пределов отбрасывается, а не губит весь список. `dropped` — сколько записей
 * пропущено (для журнала). */
export function desktopFamilies(json: unknown): { families: FontFamilyDto[]; dropped: number } {
  if (!Array.isArray(json)) return { families: [], dropped: 0 };
  const out: FontFamilyDto[] = [];
  let dropped = Math.max(0, json.length - MAX_FONT_FAMILIES);
  for (const f of json.slice(0, MAX_FONT_FAMILIES) as unknown[]) {
    const fam = f as { family?: unknown; faces?: unknown } | null;
    if (!isName(fam?.family) || !Array.isArray(fam.faces)) {
      dropped++;
      continue;
    }
    dropped += Math.max(0, fam.faces.length - MAX_FONT_FACES);
    const faces: FontFamilyDto['faces'] = [];
    for (const x of fam.faces.slice(0, MAX_FONT_FACES) as unknown[]) {
      const face = x as Record<string, unknown> | null;
      if (!face || !isName(face.style) || typeof face.file !== 'string' || !/^\d{1,9}$/.test(face.file)) {
        dropped++;
        continue;
      }
      faces.push({
        style: face.style,
        weight: typeof face.weight === 'number' ? face.weight : undefined,
        italic: typeof face.italic === 'boolean' ? face.italic : undefined,
        stretch: typeof face.stretch === 'number' ? face.stretch : undefined,
        file: face.file,
        index: typeof face.index === 'number' && Number.isInteger(face.index) && face.index >= 0 ? face.index : undefined,
      });
    }
    if (faces.length) out.push({ family: fam.family, faces });
    else dropped++;
  }
  return { families: out, dropped };
}

/** Ключ начертания как в ядре (`style_key`): без разделителей и регистра. */
export function styleKey(s: string): string {
  return [...s].filter((c) => /[\p{L}\p{N}]/u.test(c)).join('').toLowerCase();
}

/** Начертание замены по умолчанию: то же имя, иначе Regular, иначе первое. */
export function defaultStyle(choice: FontChoice | undefined, wanted: string): string {
  if (!choice || choice.styles.length === 0) return '';
  return (
    choice.styles.find((s) => styleKey(s) === styleKey(wanted)) ??
    choice.styles.find((s) => styleKey(s) === 'regular') ??
    choice.styles[0]
  );
}
