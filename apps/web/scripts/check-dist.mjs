// После `vite build`: обе страницы на месте. Без debug.html в сборке сервер отдаёт
// на его адрес index.html (запасной маршрут), и тесты ядра шли бы не на стенде.
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const dist = fileURLToPath(new URL('../dist/', import.meta.url));
for (const [file, marker] of [
  ['index.html', '<title>Ferrum</title>'],
  ['debug.html', '<title>Ferrum — стенд ядра</title>'],
]) {
  if (!existsSync(dist + file)) {
    console.error(`dist/${file} не собран: проверьте build.rolldownOptions.input в vite.config.ts`);
    process.exit(1);
  }
  if (!readFileSync(dist + file, 'utf8').includes(marker)) {
    console.error(`dist/${file} не та страница (нет ${marker})`);
    process.exit(1);
  }
}
