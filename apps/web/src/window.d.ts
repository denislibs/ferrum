// Стенд и редактор кладут ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { Scheduler } from './ui/scheduler';
import type { CoreEvent } from './wasm/ferrum_wasm';
import type { UiState } from './ui/store';
import type { FileStore, FileStoreTestHooks } from './ui/file-store';

declare global {
  interface Window {
    /** Тестовый режим: хранилище файлов (и хуки worker'а ввода-вывода — засев, отказ записи). */
    __ferrumFiles?: { store: FileStore & FileStoreTestHooks };
    __ferrum?: {
      /** Только в тестовом режиме (`__ferrumTest`, `?debug`), на стенде и с
       * `__ferrumPerf` (замеры); в обычном режиме редактора — нет. E2E проверяет
       * его наличие при открытии (`helpers.ts`, `expectCore`). */
      core?: Core;
      /** Планировщик интерфейса — там же, где `core` (кроме стенда). */
      scheduler?: Scheduler;
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
