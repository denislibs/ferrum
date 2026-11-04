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
    server: { headers },
    preview: { headers },
    worker: { format: 'es' as const },
    build: {
      target: 'es2022',
      rolldownOptions: { input },
    },
  };
});
