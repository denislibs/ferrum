// Полоса аватаров без интерфейса (спека подсистемы 4, раздел 9): участники по людям, инициалы и подпись кнопки.

import type { Participant } from '../../core/sync-client';

export interface Person {
  user: string;
  name: string;
  color: string;
  /** Участник человека, за которым следовать: новейшее соединение. */
  participant: number;
  /** Все участники человека: за кем из них следуем. */
  ids: number[];
  elsewhere: boolean;
}

/** По человеку — один, сколько бы вкладок у него ни было; себя нет. Человек на другой странице — если все его вкладки там. */
export function groupPeople(list: Participant[], self: string, currentPage: string): Person[] {
  const by = new Map<string, Person>();
  for (const p of list) {
    if (p.user === self) continue;
    const was = by.get(p.user);
    const elsewhere = p.page !== null && p.page !== currentPage;
    if (!was) {
      by.set(p.user, { user: p.user, name: p.name, color: p.color, participant: p.id, ids: [p.id], elsewhere });
    } else {
      was.ids.push(p.id);
      if (p.id > was.participant) was.participant = p.id;
      was.elsewhere &&= elsewhere;
    }
  }
  return [...by.values()];
}

const graphemes = typeof Intl !== 'undefined' && 'Segmenter' in Intl ? new Intl.Segmenter(undefined, { granularity: 'grapheme' }) : null;

/** Первый видимый символ слова: эмодзи, буква с диакритикой, суррогатная пара — целиком. */
function firstGrapheme(word: string): string {
  if (graphemes) {
    for (const s of graphemes.segment(word)) return s.segment;
    return '';
  }
  return Array.from(word)[0] ?? '';
}

/** Инициалы: первые символы двух первых слов имени, у одного слова — первый; пустое имя — «?». */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  const letters = parts.length > 1 ? firstGrapheme(parts[0]) + firstGrapheme(parts[1]) : firstGrapheme(parts[0]);
  return letters.toUpperCase();
}

/** Подпись кнопки аватара: имя и где человек. Следование — в `aria-pressed`, не в подписи: переключатель называется одинаково в
 * обоих состояниях (поведение). */
export function avatarLabel(p: Pick<Person, 'name' | 'elsewhere'>): string {
  return `${p.name}${p.elsewhere ? ' — на другой странице' : ''}`;
}
