import { defineConfig } from '@playwright/test';

// Модульные тесты чистой логики интерфейса (планировщик, поле числа, цвет,
// раскладка, буфер обмена) — тем же раннером Playwright, без браузера и сервера.
export default defineConfig({
  testDir: 'unit',
  testMatch: '**/*.test.ts',
  reporter: [['list']],
});
