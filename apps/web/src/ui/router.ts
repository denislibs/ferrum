// Маршруты приложения в адресе после `#` (назад и вперёд браузера работают):
// - `#/` (и пустой) — экран файлов, «Недавние»; `#/drafts`, `#/trash` — другие разделы;
// - `#/file/<id>` — редактор с файлом из хранилища;
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`).
// Параметры поиска (`?nodes=`, `?backend=` — тесты) маршрутизатор не трогает.

import type { Section } from '../core/file-list';
import { MAX_ID_BYTES } from '../core/file-list';

export type Route = { kind: 'files'; section: Section } | { kind: 'file'; id: string } | { kind: 'new' };

const SECTION_PATH: Record<Section, string> = { recent: '', drafts: 'drafts', trash: 'trash' };

/** Идентификатор файла из адреса — не доверенный: печатные ASCII без `/`, не длиннее предела. */
function validId(id: string): boolean {
  return id.length > 0 && new TextEncoder().encode(id).length <= MAX_ID_BYTES && /^[A-Za-z0-9_-]+$/.test(id);
}

/** Маршрут по части адреса после `#` (с решёткой или без); неизвестное — экран файлов. */
export function parseRoute(hash: string): Route {
  const path = hash.replace(/^#/, '').replace(/^\/+/, '').replace(/\/+$/, '');
  if (path === 'new') return { kind: 'new' };
  if (path === 'drafts') return { kind: 'files', section: 'drafts' };
  if (path === 'trash') return { kind: 'files', section: 'trash' };
  const m = /^file\/([^/]+)$/.exec(path);
  if (m) {
    let id = '';
    try {
      id = decodeURIComponent(m[1]);
    } catch {
      id = '';
    }
    // Неверный id — тот же путь, что и неизвестный: редактор сообщит «Файл не найден».
    return { kind: 'file', id: validId(id) ? id : '' };
  }
  return { kind: 'files', section: 'recent' };
}

export function routeHash(r: Route): string {
  switch (r.kind) {
    case 'new':
      return '#/new';
    case 'file':
      return `#/file/${encodeURIComponent(r.id)}`;
    case 'files':
      return `#/${SECTION_PATH[r.section]}`;
  }
}

export function sameRoute(a: Route, b: Route): boolean {
  return routeHash(a) === routeHash(b);
}

/** Адрес страницы с маршрутом `r` (поиск — `?nodes=` и т. п. — сохраняется). */
export function routeUrl(r: Route): string {
  return `${location.pathname}${location.search}${routeHash(r)}`;
}

/** Номер записи истории, который приложение кладёт в `history.state`; `null` —
 * запись не наша (адрес набран вручную, первая загрузка). */
export function historyIndex(state: unknown): number | null {
  const i = (state as { ferrumIndex?: unknown } | null)?.ferrumIndex;
  return typeof i === 'number' && Number.isSafeInteger(i) ? i : null;
}
