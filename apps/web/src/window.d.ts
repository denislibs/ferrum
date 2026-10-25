// Стенд и редактор кладут ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { CoreEvent } from './wasm/ferrum_wasm';
import type { UiState } from './ui/store';

declare global {
  interface Window {
    __ferrum?: {
      /** Только в тестовом режиме (`__ferrumTest`, `?debug`), на стенде и с
       * `__ferrumPerf` (замеры); в обычном режиме редактора — нет. */
      core: Core;
      events: CoreEvent[];
      /** Редактор, тестовый режим: тип, размер байтов и текст ошибки каждого события. */
      uiEvents?: { type: string; size?: number; message?: string }[];
      /** Документ открыт (событие `opened`). */
      ready: boolean;
      /** Редактор: состояние интерфейса (только чтение в тестах). */
      ui?: UiState;
    };
  }
}
