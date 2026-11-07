// Чистая логика панели и окна комментариев (спека подсистемы 4, раздел 10.3): фильтры и порядок панели, окно виртуального
// списка, упоминания в поле ввода. Без DOM: модульные тесты проверяют её отдельно.

import {
  cleanCommentText,
  commentLength,
  COMMENT_BODY_MAX,
  MENTION_QUERY_MAX,
  normalizeCommentBody,
  type CommentAuthor,
  type ThreadView,
} from '@ferrum/protocol';

export interface PanelFilter {
  /** `page` — только текущая страница. */
  scope: 'page' | 'all';
  /** `date` — новые сверху; `unread` — сначала непрочитанные, внутри — новые сверху. */
  sort: 'date' | 'unread';
  /** «Только мои»: начал, ответил или упомянут — участник обсуждения. */
  mine: boolean;
  /** Показ закрытых; по умолчанию скрыты, как в Figma **(поведение)**. */
  resolved: boolean;
}

export const DEFAULT_FILTER: PanelFilter = { scope: 'page', sort: 'date', mine: false, resolved: false };

/** Обсуждения панели по фильтру. Порядок устойчивый: при равном времени — по номеру. */
export function panelThreads(list: Iterable<ThreadView>, f: PanelFilter, page: string, me: string): ThreadView[] {
  const out: ThreadView[] = [];
  for (const t of list) {
    if (f.scope === 'page' && t.pin.page !== page) continue;
    if (!f.resolved && t.resolvedAt !== null) continue;
    if (f.mine && !t.participants.includes(me) && t.author.id !== me) continue;
    out.push(t);
  }
  out.sort((a, b) => {
    if (f.sort === 'unread' && a.unread !== b.unread) return a.unread ? -1 : 1;
    return b.lastAt - a.lastAt || b.seq - a.seq;
  });
  return out;
}

/** Окно виртуального списка: какие строки рисовать при прокрутке `top` и высоте `height`; запас — строк сверху и снизу. */
export function listWindow(top: number, height: number, row: number, total: number, spare = 4): { start: number; end: number } {
  const start = Math.min(total, Math.max(0, Math.floor(top / row) - spare));
  const end = Math.min(total, Math.ceil((top + height) / row) + spare);
  return { start, end: Math.max(start, end) };
}

/** Подпись автора: имя; без имени — «Участник»; удалённая учётная запись — «Удалённый пользователь» **(поведение)**. */
export function authorName(a: CommentAuthor | null): string {
  if (!a || a.id === null) return 'Удалённый пользователь';
  return a.name.trim() || 'Участник';
}

/** Имя в упоминании: известное без имени — «Участник», как у автора; не из списка упомянутых — «неизвестный». */
export function mentionName(name: string | null | undefined): string {
  if (name === null || name === undefined) return 'неизвестный';
  return name.trim() || 'Участник';
}

/** Упоминание, которое пишут сейчас: `@` и текст до каретки без пробелов. `null` — не пишут. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const m = /(^|\s)@([^\s@]*)$/.exec(before);
  if (!m || m[2].length > MENTION_QUERY_MAX) return null;
  return { start: caret - m[2].length - 1, query: m[2] };
}

/** Выбранный человек вставляется как `@Имя ` на место набранного упоминания. */
export function insertMention(text: string, start: number, caret: number, name: string): { text: string; caret: number } {
  const label = `@${name} `;
  return { text: text.slice(0, start) + label + text.slice(caret), caret: start + label.length };
}

/** Выбранный человек запоминается вместе с текстом: на место `start` вставлено `@name`. Одинаковые имена (например, у двоих нет
 * имени — «Участник») различает только порядок: запись встаёт после стольких записей с этим именем, сколько `@name` в тексте
 * до `start`; один и тот же человек, упомянутый дважды, — две записи. */
export function addPicked(
  picked: { id: string; name: string }[],
  text: string,
  start: number,
  p: { id: string; name: string },
): { id: string; name: string }[] {
  const label = `@${p.name}`;
  let before = 0;
  for (let i = text.indexOf(label); i !== -1 && i < start; i = text.indexOf(label, i + 1)) before++;
  const out = [...picked];
  let seen = 0;
  let at = out.length;
  for (let i = 0; i < out.length; i++) {
    if (out[i].name !== p.name) continue;
    if (seen === before) {
      at = i;
      break;
    }
    seen++;
  }
  out.splice(at, 0, p);
  return out;
}

/** Текст для сервера: `@Имя` выбранных людей — `@[userId]`; остальное как есть. Длинные имена первыми: «Анна Мария» не станет
 * «Анна» с хвостом. Имя, которое человек стёр или изменил, остаётся текстом. Люди с одинаковым именем — по порядку в тексте. */
export function storedBody(text: string, picked: { id: string; name: string }[]): string {
  const sorted = [...picked].sort((a, b) => b.name.length - a.name.length);
  const used = new Map<string, number>();
  let out = '';
  let i = 0;
  while (i < text.length) {
    const named = text[i] === '@' ? sorted.filter((p) => text.startsWith(`@${p.name}`, i)) : [];
    let hit: { id: string; name: string } | undefined;
    if (named.length > 0) {
      const same = named.filter((p) => p.name === named[0].name);
      const n = used.get(named[0].name) ?? 0;
      used.set(named[0].name, n + 1);
      hit = same[Math.min(n, same.length - 1)];
    }
    if (hit) {
      out += `@[${hit.id}]`;
      i += hit.name.length + 1;
    } else out += text[i++];
  }
  return out;
}

/** Поле правки своего комментария: `@[id]` — обратно в `@Имя` и выбранные (по записи на каждое упоминание, в порядке текста). */
export function editableBody(body: string, names: Record<string, string>): { text: string; picked: { id: string; name: string }[] } {
  const picked: { id: string; name: string }[] = [];
  const text = body.replace(/@\[([A-Za-z0-9_-]{1,64})\]/g, (all, id: string) => {
    // Не из списка упомянутых — остаётся `@[id]`: при сохранении это то же упоминание. Без имени — «Участник», как при вставке.
    if (!Object.hasOwn(names, id)) return all;
    const name = mentionName(names[id]);
    picked.push({ id, name });
    return `@${name}`;
  });
  return { text, picked };
}

/** Можно ли отправить — та же функция, что у сервера (`normalizeCommentBody`): текст из одних невидимых знаков пуст, длина —
 * в кодовых точках после той же очистки. */
export function bodyState(stored: string): { ok: boolean; length: number; over: boolean } {
  const length = commentLength(cleanCommentText(stored));
  return { ok: normalizeCommentBody(stored) !== null, length, over: length > COMMENT_BODY_MAX };
}

/** Время для панели и окна: «только что», минуты, часы, иначе дата. */
export function timeLabel(at: number, now: number): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 60) return 'только что';
  if (s < 3600) return `${Math.floor(s / 60)} мин назад`;
  if (s < 86_400) return `${Math.floor(s / 3600)} ч назад`;
  return new Date(at).toLocaleDateString('ru', { day: 'numeric', month: 'short' });
}

/** Окно у метки: справа сверху от неё (`anchor` — точка метки в координатах холста), целиком в пределах холста `box` с
 * отступом `gap`. Не помещается — прижато к левому и верхнему краю (длинное прокручивается внутри). */
export function popoverPlace(
  anchor: { x: number; y: number },
  box: { left: number; top: number; width: number; height: number },
  size: { width: number; height: number },
  gap = 8,
): { left: number; top: number } {
  const left = Math.min(box.left + anchor.x + 36, box.left + box.width - size.width - gap);
  const top = Math.min(box.top + anchor.y - 28, box.top + box.height - size.height - gap);
  return { left: Math.max(box.left + gap, left), top: Math.max(box.top + gap, top) };
}
