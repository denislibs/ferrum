// После `vite build`: сборка того режима, что заказан. Автономная — обе страницы (без
// debug.html сервер отдаёт на его адрес index.html запасным маршрутом, и тесты ядра шли бы
// не на стенде) и признак режима в коде. Рабочая — только редактор и никакого кода
// автономного режима: иначе редактор открывался бы без входа.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Метка, которую оставляет только ветка автономного режима в `main.tsx`. */
export const LOCAL_MARKER = 'ferrumLocal';
/** Адреса входа: код входа есть только в рабочей сборке. */
export const AUTH_MARKER = '/api/auth/';
/** Команда `TauriFileStore`: в рабочей сборке её нет. */
export const TAURI_STORE_MARKER = 'files_open_dialog';
/** Тестовый доступ к локальному хранилищу. */
export const TEST_STORE_MARKER = '__ferrumFiles';
/** Адреса API файлов: код сервера есть только в рабочей сборке. */
export const FILES_MARKER = '/api/files';
/** Окно комментариев: класс есть и в коде, и в стилях (`ui/server/comments.tsx`, `styles/comments.css`) — только в рабочей
 * сборке. Не `fe-comments`: место панели (`fe-comments-host`) — общий клей `app.tsx`, он есть в обеих. */
export const COMMENTS_MARKER = 'fe-thread';

/** Проблема сборки словами или `null`. `mode` — `standalone` или `server`. */
export function checkDist(dist, mode) {
  const pages = [['index.html', '<title>Ferrum</title>']];
  if (mode === 'standalone') pages.push(['debug.html', '<title>Ferrum — стенд ядра</title>']);
  for (const [file, marker] of pages) {
    if (!existsSync(join(dist, file))) return `${file} не собран: проверьте build.rolldownOptions.input в vite.config.ts`;
    if (!readFileSync(join(dist, file), 'utf8').includes(marker)) return `${file} не та страница (нет ${marker})`;
  }
  if (mode === 'server' && existsSync(join(dist, 'debug.html'))) return 'debug.html в рабочей сборке: стенд ядра открывался бы без входа';
  const assets = join(dist, 'assets');
  const listed = existsSync(assets) ? readdirSync(assets) : [];
  const inFiles = (ext) => (marker) => listed.some((f) => f.endsWith(ext) && readFileSync(join(assets, f), 'utf8').includes(marker));
  const has = inFiles('.js');
  const hasCss = inFiles('.css');
  const local = has(LOCAL_MARKER);
  const auth = has(AUTH_MARKER);
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'standalone' && auth) return 'в автономной сборке есть код входа: проверьте условия по VITE_FERRUM_LOCAL';
  if (mode === 'standalone' && has(FILES_MARKER)) return 'в автономной сборке есть код файлов на сервере: проверьте импорты `ui/server`';
  if (mode === 'standalone' && (has(COMMENTS_MARKER) || hasCss(COMMENTS_MARKER)))
    return 'в автономной сборке есть комментарии (код или стили): проверьте импорты `ui/server/comments`';
  if (mode === 'server' && !has(FILES_MARKER)) return 'в рабочей сборке нет кода файлов на сервере';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  if (mode === 'server' && !auth) return 'в рабочей сборке нет кода входа';
  if (mode === 'server' && has(TAURI_STORE_MARKER)) return 'в рабочей сборке есть хранилище десктопа: проверьте импорты `tauri.ts`';
  if (mode === 'server' && has(TEST_STORE_MARKER)) return 'в рабочей сборке есть тестовый доступ к хранилищу';
  if (mode === 'server' && !(has(COMMENTS_MARKER) && hasCss(COMMENTS_MARKER))) return 'в рабочей сборке нет комментариев (кода или стилей)';
  return null;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const mode = process.argv[2];
  if (mode !== 'standalone' && mode !== 'server') {
    console.error('Использование: check-dist.mjs standalone|server');
    process.exit(1);
  }
  const problem = checkDist(fileURLToPath(new URL(mode === 'server' ? '../dist-server/' : '../dist/', import.meta.url)), mode);
  if (problem) {
    console.error(`dist: ${problem}`);
    process.exit(1);
  }
}
