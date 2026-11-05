// Файлы автономной сборки и десктопа: на диске через Tauri, в браузере — IndexedDB в worker'е.
// Этого кода нет в рабочей сборке (её ветка `main.tsx` берёт файлы с сервера, B-206).

import { isTestMode } from './boot';
import { IdbFileStore, type FileStore } from './file-store';
import type { SettingsBackend } from './settings-store';
import { isTauri, TauriFileStore } from './tauri';

export function createLocalStore(): FileStore & SettingsBackend {
  if (isTauri()) return new TauriFileStore();
  const test = isTestMode();
  const store = new IdbFileStore(test);
  // Тестам — хранилище напрямую; `?debug` в рабочей сборке этого не открывает (B-205).
  if (test) window.__ferrumFiles = { store };
  return store;
}
