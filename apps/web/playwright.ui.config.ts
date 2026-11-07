import { defineConfig } from '@playwright/test';

// Интерфейс рабочей сборки без стенда (`e2e-ui`): компоненты `ui/server` монтируются на странице `vite` в режиме разработки,
// ядро и `sync.worker` подменены. Ни wasm, ни Docker не нужны; порт свой — 4173/4174 у автономного конфига.
const PORT = 4183;

export default defineConfig({
  testDir: 'e2e-ui',
  timeout: 60_000,
  reporter: [['list']],
  use: { baseURL: `http://localhost:${PORT}`, viewport: { width: 1280, height: 720 } },
  webServer: { command: `npx vite --port ${PORT} --strictPort`, port: PORT, reuseExistingServer: false },
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
});
