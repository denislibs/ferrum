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

// Две страницы: редактор (index.html, интерфейс на SolidJS) и стенд этапа 4
// (debug.html: холст на всё окно и отладочная плашка) — для тестов ядра.
export default defineConfig({
  plugins: [solid()],
  server: { headers },
  preview: { headers },
  worker: { format: 'es' },
  build: {
    target: 'es2022',
    rolldownOptions: { input: { main: 'index.html', debug: 'debug.html' } },
  },
});
