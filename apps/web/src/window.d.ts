// Стенд кладёт ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { CoreEvent } from './wasm/ferrum_wasm';

declare global {
  interface Window {
    __ferrum?: {
      core: Core;
      events: CoreEvent[];
      /** Документ открыт (событие `opened`). */
      ready: boolean;
    };
  }
}
