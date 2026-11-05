// Маршруты приложения в адресе после `#` (назад и вперёд браузера работают):
// - `#/` (и пустой) — экран файлов, «Недавние»; `#/drafts`, `#/trash` — другие разделы;
// - `#/file/<id>` — редактор с файлом из хранилища; `#/file/<id>?page=<pageId>` —
//   с этой страницей («Копировать ссылку на страницу»);
// - `#/new` — создать файл в хранилище и открыть его (адрес заменяется на `#/file/<id>`);
//   `#/new?project=<id>` — в этом проекте, где проекты есть;
// - `#/file/<id>?link=<токен>` — файл по ссылке «Поделиться», токен из адреса сразу убирается.
// Другие разделы (проекты команды, «Доступные мне») — там, где они есть: `extensions.parseSection`.
// Параметры поиска (`?nodes=`, `?backend=` — тесты) маршрутизатор не трогает.

import type { Section } from '../core/file-list';
import { MAX_ID_BYTES } from '../core/file-list';
import { extensions } from './extensions';

export type Route =
  { kind: 'files'; section: Section } | { kind: 'file'; id: string; page?: string; link?: string } | { kind: 'new'; project?: string };

const SECTION_PATH: Partial<Record<Section, string>> = { recent: '', drafts: 'drafts', trash: 'trash', shared: 'shared' };
const sectionPath = (s: Section): string => SECTION_PATH[s] ?? s.replace(':', '/');

/** Идентификатор файла из адреса — не доверенный: печатные ASCII без `/`, не длиннее предела. */
function validId(id: string): boolean {
  return id.length > 0 && new TextEncoder().encode(id).length <= MAX_ID_BYTES && /^[A-Za-z0-9_-]+$/.test(id);
}

/** Идентификатор страницы из адреса — не доверенный: `счётчик@узел` Loro, не длиннее 64. */
function validPage(id: string): boolean {
  return id.length <= 64 && /^\d+@\d+$/.test(id);
}

/** Маршрут по части адреса после `#` (с решёткой или без); неизвестное — экран файлов. */
export function parseRoute(hash: string): Route {
  const [rawPath, query = ''] = hash.replace(/^#/, '').split('?', 2);
  const path = rawPath.replace(/^\/+/, '').replace(/\/+$/, '');
  if (path === 'new') {
    const project = new URLSearchParams(query).get('project');
    return project !== null && validId(project) ? { kind: 'new', project } : { kind: 'new' };
  }
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
    const route: Route = { kind: 'file', id: validId(id) ? id : '' };
    // Неверная страница отбрасывается: файл откроется на первой.
    const page = new URLSearchParams(query).get('page');
    if (page !== null && validPage(page)) route.page = page;
    const link = new URLSearchParams(query).get('link');
    if (link !== null && /^[A-Za-z0-9_-]{1,64}$/.test(link)) route.link = link;
    return route;
  }
  const other = extensions.parseSection?.(path);
  return { kind: 'files', section: other ?? 'recent' };
}

export function routeHash(r: Route): string {
  switch (r.kind) {
    case 'new':
      return r.project ? `#/new?project=${encodeURIComponent(r.project)}` : '#/new';
    case 'file':
      return `#/file/${encodeURIComponent(r.id)}${r.page ? `?page=${encodeURIComponent(r.page)}` : ''}`;
    case 'files':
      return `#/${sectionPath(r.section)}`;
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
