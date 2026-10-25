// Стенд и редактор кладут ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { CoreEvent } from './wasm/ferrum_wasm';
import type { UiState } from './ui/store';

declare global {
  interface Window {
    __ferrum?: {
      core: Core;
      events: CoreEvent[];
      /** Документ открыт (событие `opened`). */
      ready: boolean;
      /** Редактор: состояние интерфейса (только чтение в тестах). */
      ui?: UiState;
    };
  }
}
