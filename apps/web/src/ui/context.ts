// Общее для компонентов редактора: ядро, состояние, планировщик, платформа.

import { createContext, useContext } from 'solid-js';
import type { Command } from '../wasm/ferrum_wasm';
import type { Core, Platform } from '../core/core';
import type { Scheduler } from './scheduler';
import type { LayoutMap } from './shortcuts';
import type { UiStore } from './store';

export interface Editor {
  core: Core;
  store: UiStore;
  scheduler: Scheduler;
  platform: Platform;
  send(cmd: Command): void;
  /** Команда редактора по ключу таблицы (`edit.undo`). */
  run(command: string): void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
}

export const EditorContext = createContext<Editor>();

export function useEditor(): Editor {
  const e = useContext(EditorContext);
  if (!e) throw new Error('Компонент редактора вне EditorContext');
  return e;
}
