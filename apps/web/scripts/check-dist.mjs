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
  const code = existsSync(assets) ? readdirSync(assets).filter((f) => f.endsWith('.js')) : [];
  const has = (marker) => code.some((f) => readFileSync(join(assets, f), 'utf8').includes(marker));
  const local = has(LOCAL_MARKER);
  const auth = has(AUTH_MARKER);
  if (mode === 'standalone' && !local) return 'в автономной сборке нет автономного режима: проверьте VITE_FERRUM_LOCAL (.env.standalone)';
  if (mode === 'standalone' && auth) return 'в автономной сборке есть код входа: проверьте условия по VITE_FERRUM_LOCAL';
  if (mode === 'server' && local) return 'в рабочей сборке есть код автономного режима: VITE_FERRUM_LOCAL задан при сборке?';
  if (mode === 'server' && !auth) return 'в рабочей сборке нет кода входа';
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
