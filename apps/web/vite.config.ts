import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

// Изоляция (crossOriginIsolated) нужна для SharedArrayBuffer. FERRUM_NO_ISOLATION=1 —
// сервер без заголовков: проверка запасного транспорта на postMessage.
const isolated = process.env.FERRUM_NO_ISOLATION !== '1';
const headers: Record<string, string> = isolated
  ? {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'credentialless',
    }
  : {};

// Автономная сборка (`--mode standalone`) — две страницы: редактор (index.html) и стенд
// этапа 4 (debug.html: холст на всё окно и отладочная плашка) для тестов ядра. Рабочая
// сборка — только редактор: стенд открывался бы без входа.
export default defineConfig(({ mode }) => {
  const input: Record<string, string> = { main: 'index.html' };
  if (mode === 'standalone') input.debug = 'debug.html';
  return {
    plugins: [solid()],
    // Автономная сборка не видит кода сервера вовсе: условие в `main.tsx` убирает ветку, но не файл worker'а.
    resolve: {
      alias:
        mode === 'standalone'
          ? [{ find: /^.*\/ui\/server\/setup$/, replacement: fileURLToPath(new URL('./src/ui/server/setup.local.ts', import.meta.url)) }]
          : [],
    },
    // `npm run dev:server`: API — у сервера из Docker (PUBLIC_URL должен быть адресом vite).
    server: { headers, fs: { allow: ['../..'] }, proxy: { '/api': process.env.FERRUM_API ?? 'http://localhost:8090' } },
    preview: { headers },
    worker: { format: 'es' as const },
    build: {
      target: 'es2022',
      rolldownOptions: { input },
    },
  };
});
