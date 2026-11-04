// Раздача собранного `apps/web` и заголовки, общие для всех ответов сервера. Заголовки
// изоляции нужны для `SharedArrayBuffer`, как у `vite preview`; файлы с хэшем в имени
// (`assets/`) кэшируются навсегда, остальное — с проверкой.

import { resolve, sep } from 'node:path';

export const SECURITY_HEADERS: Record<string, string> = {
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

export function withHeaders(res: Response): Response {
  for (const [k, v] of Object.entries(SECURITY_HEADERS)) res.headers.set(k, v);
  return res;
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
  const root = resolve(dist);
  const full = resolve(root, `.${path === '/' ? '/index.html' : path}`);
  if (!full.startsWith(root + sep)) return new Response('Не найдено', { status: 404 });
  const file = Bun.file(full);
  // Маршруты приложения — после `#`: запасной index.html не нужен, чужой адрес — честный 404.
  if (!(await file.exists())) return new Response('Не найдено', { status: 404 });
  const headers: Record<string, string> = {
    'Cache-Control': path.startsWith('/assets/') ? 'public, max-age=31536000, immutable' : 'no-cache',
    'Content-Security-Policy': CSP,
  };
  return new Response(req.method === 'HEAD' ? null : file, {
    headers: { 'Content-Type': file.type, 'Content-Length': String(file.size), ...headers },
  });
}
