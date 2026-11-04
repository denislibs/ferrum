// Раздача собранного `apps/web` и заголовки, общие для всех ответов сервера. Заголовки
// изоляции нужны для `SharedArrayBuffer`, как у `vite preview`; файлы с хэшем в имени
// (`assets/`) кэшируются навсегда, остальное (в том числе шрифты) — с проверкой по `ETag`:
// неизменившийся файл даёт 304 без тела.

import { join, resolve, sep } from 'node:path';

const SECURITY_HEADERS: Record<string, string> = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'credentialless',
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'same-origin',
};

/** Как в окне десктопа (`tauri.conf.json`): только свои скрипты, wasm и worker'ы. */
const CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob: data:",
  "font-src 'self' data:",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'self'",
  "frame-ancestors 'none'",
].join('; ');

/** Заголовки для всех ответов. HSTS — только при `https://`: браузер запомнит его на год, поэтому
 * без поддоменов, они могут жить без HTTPS. */
export function responseHeaders(publicUrl: string): (res: Response) => Response {
  const headers = publicUrl.startsWith('https://')
    ? { ...SECURITY_HEADERS, 'Strict-Transport-Security': 'max-age=31536000' }
    : SECURITY_HEADERS;
  return (res) => {
    for (const [k, v] of Object.entries(headers)) res.headers.set(k, v);
    return res;
  };
}

/** Копия браузера ещё верна? `If-None-Match` главнее даты (RFC 9110). */
function notModified(req: Request, etag: string, lastModified: number): boolean {
  const match = req.headers.get('if-none-match');
  if (match !== null) return match.trim() === '*' || match.split(',').some((v) => v.trim().replace(/^W\//, '') === etag);
  const since = Date.parse(req.headers.get('if-modified-since') ?? '');
  // В заголовке дата с точностью до секунды.
  return !Number.isNaN(since) && Math.floor(lastModified / 1000) * 1000 <= since;
}

export async function serveStatic(req: Request, dist: string): Promise<Response> {
  if (req.method !== 'GET' && req.method !== 'HEAD')
    return new Response('Метод не разрешён', { status: 405, headers: { Allow: 'GET, HEAD' } });
  let path: string;
  try {
    path = decodeURIComponent(new URL(req.url).pathname);
  } catch {
    return new Response('Неверный адрес', { status: 400 });
  }
  // `\0` файловая система не примет: `Bun.file` бросил бы исключение на анонимный запрос.
  if (path.includes('\0')) return new Response('Не найдено', { status: 404 });
  const root = resolve(dist);
  const full = resolve(root, `.${path === '/' ? '/index.html' : path}`);
  if (!full.startsWith(root + sep)) return new Response('Не найдено', { status: 404 });
  const file = Bun.file(full);
  // Маршруты приложения — после `#`: запасной index.html не нужен, чужой адрес — честный 404.
  if (!(await file.exists())) return new Response('Не найдено', { status: 404 });
  const headers: Record<string, string> = {
    // По настоящему пути файла: `/assets/..%2findex.html` — это `index.html`.
    'Cache-Control': full.startsWith(join(root, 'assets') + sep) ? 'public, max-age=31536000, immutable' : 'no-cache',
    'Content-Security-Policy': CSP,
    ETag: `"${file.size.toString(36)}-${file.lastModified.toString(36)}"`,
    'Last-Modified': new Date(file.lastModified).toUTCString(),
  };
  if (notModified(req, headers.ETag, file.lastModified)) return new Response(null, { status: 304, headers });
  return new Response(req.method === 'HEAD' ? null : file, {
    headers: { 'Content-Type': file.type, 'Content-Length': String(file.size), ...headers },
  });
}
