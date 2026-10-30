// Шрифты на стороне TypeScript (спека подсистемы 3, раздел 4): разбор манифеста
// вшитых шрифтов, адрес файла вшитого шрифта и список системных шрифтов Chrome
// (`queryLocalFonts`) в виде команды `fontsAvailable`. Чистая логика — для
// модульных тестов; данные не доверенные и ограничены как в ядре.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
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
