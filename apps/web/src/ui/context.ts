// Общее для компонентов редактора: ядро, состояние, планировщик, платформа.

import { createContext, useContext } from 'solid-js';
import type { Command, ContextTarget, MenuActionDto } from '../wasm/ferrum_wasm';
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
  /** Команда редактора по ключу таблицы (`edit.undo`). Команды интерфейса
   * (`ui.*`) выполняются сразу здесь (`uiCommand`), без круга через ядро: выбор
   * файла требует, чтобы его вызвал жест пользователя. */
  run(command: string): void;
  /** Исполнитель команд интерфейса (`ui.*`): ставит редактор приложения; ядро
   * сообщает о них событием `uiCommand` (сочетание клавиш на холсте). */
  uiCommand?: (key: string) => void;
  /** Открыть контекстное меню цели у точки окна `at` (спека подсистемы 2, раздел
   * 5): ядро выбирает на правой кнопке и присылает пункты. `onAction` — действия
   * над целью (переименовать, страницы). Ставит редактор приложения. */
  contextMenu?: (target: ContextTarget, at: { x: number; y: number }, label: string, onAction?: (a: MenuActionDto) => void) => void;
  /** Команда выполнена из меню или «Быстрых действий»: запомнить в недавних.
   * Ставит редактор приложения. */
  noteRecent?: (key: string) => void;
  /** Раскладка клавиатуры пользователя для подписей (если браузер её отдаёт); сигнал. */
  layout(): LayoutMap | undefined;
  /** «Показать системные шрифты» (Chrome — с разрешения браузера). */
  showSystemFonts(): void;
  /** Остановить ядро (уход с редактора). */
  dispose(): void;
}

export const EditorContext = createContext<Editor>();

export function useEditor(): Editor {
  const e = useContext(EditorContext);
  if (!e) throw new Error('Компонент редактора вне EditorContext');
  return e;
}
