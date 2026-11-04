import { defineConfig } from '@playwright/test';

// Проект `server`: рабочая сборка против настоящего стенда в Docker (свой проект compose и порты,
// письма — Mailpit). Стенд поднимает `e2e-server/global-setup.ts` и снимает `global-teardown.ts`;
// `FERRUM_KEEP_STAND=1` оставляет его для повторных прогонов. Отдельный конфиг: обычному
// `playwright test` Docker не нужен.
export const STAND_URL = 'http://localhost:18090';

export default defineConfig({
  testDir: 'e2e-server',
  timeout: 60_000,
  // Тесты делят один стенд и очищают его счётчики.
  workers: 1,
  reporter: [['list']],
  globalSetup: './e2e-server/global-setup.ts',
  globalTeardown: './e2e-server/global-teardown.ts',
  use: { baseURL: STAND_URL, viewport: { width: 1280, height: 720 } },
  projects: [
    { name: 'server', use: { browserName: 'chromium' } },
    { name: 'server-firefox', use: { browserName: 'firefox' } },
  ],
});
