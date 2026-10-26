import { existsSync } from 'node:fs';
import { defineConfig, firefox } from '@playwright/test';

// Firefox (этап 6): тесты с меткой @firefox — круг сохранения, журнал, отсутствие
// File System Access. Проект есть, только если браузер установлен
// (`npx playwright install firefox`): без него прогон не падает. В CI его нет.
const FIREFOX = existsSync(firefox.executablePath());
if (!FIREFOX) {
  // Тесты @firefox пропущены не молча: прогон говорит, почему и как их включить.
  console.warn('Firefox для Playwright не установлен — тесты @firefox пропущены (npx playwright install firefox)');
}

// Стенд собирается заранее (`npm run wasm && npm run build`); тесты идут против
// `vite preview` — того же бандла, что уйдёт пользователю. Два сервера: с
// заголовками изоляции (SharedArrayBuffer) и без них (запасной транспорт).
// Проекты: `chromium` — headless как есть: WebGPU нет, WebGL2 — программный
// SwiftShader (автовыбор падает на WebGL2); `gpu` — тот же Chromium с настоящим GPU
// (ANGLE Metal / Vulkan) и WebGPU: тесты с меткой @gpu и замеры @perf.
// Флаги `gpu` — под Metal, то есть только для macOS с видеокартой; в CI на Linux
// без GPU запускается только `chromium` (`npm run e2e:ci`) и без замеров @perf:
// на программном SwiftShader общей машины их ворота времени не о нас. Локальный
// полный прогон (`npx playwright test`) включает замеры обоих проектов.
const GPU_ARGS = ['--enable-unsafe-webgpu', '--enable-gpu', '--use-angle=metal', '--ignore-gpu-blocklist'];

export default defineConfig({
  testDir: 'e2e',
  timeout: 120_000,
  // Замеры производительности не должны делить машину с другими тестами.
  workers: 1,
  reporter: [['list']],
  use: { baseURL: 'http://localhost:4173', viewport: { width: 1280, height: 720 } },
  webServer: [
    { command: 'npx vite preview --port 4173 --strictPort', port: 4173, reuseExistingServer: false },
    {
      command: 'npx vite preview --port 4174 --strictPort',
      port: 4174,
      env: { FERRUM_NO_ISOLATION: '1' },
      reuseExistingServer: false,
    },
  ],
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' }, grepInvert: /@gpu/ },
    {
      name: 'gpu',
      use: { browserName: 'chromium', launchOptions: { args: GPU_ARGS } },
      grep: /@gpu|@perf/,
    },
    ...(FIREFOX ? [{ name: 'firefox', use: { browserName: 'firefox' as const }, grep: /@firefox/ }] : []),
  ],
});
