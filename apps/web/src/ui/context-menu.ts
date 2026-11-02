// Контекстные меню (спека подсистемы 2, раздел 5): дерево — из ядра (событие
// `contextMenu` в ответ на команду `contextMenu`), здесь — только названия из
// таблицы команд и что делать с пунктами, которых в таблице нет: узел «Выделить
// слой» и действие над целью меню (их выполняет тот, кто меню открыл). Основной
// поток ничего не вычисляет о выделении: выбор на правой кнопке и состав меню
// делает ядро, доступность команд — из `CommandStates`.

import type { ContextTarget, KeymapEntry, MenuActionDto, MenuItemDto } from '../wasm/ferrum_wasm';
import { tidy } from './main-menu';
import type { MenuItem } from './menu';

export type { ContextTarget };

/** Что делать с пунктами не из таблицы команд. */
export interface ContextHandlers {
  /** «Выделить слой» → узел. */
  select(id: string): void;
  /** Действие над целью меню (переименовать, страницы…). */
  action(action: MenuActionDto): void;
}

/** Пункты меню интерфейса из дерева ядра. Команды без записи в таблице
 * пропускаются, пустые подменю тоже. */
export function buildContextMenu(tree: readonly MenuItemDto[], entries: ReadonlyMap<string, KeymapEntry>, on: ContextHandlers): MenuItem[] {
  const out: MenuItem[] = [];
  for (const n of tree) {
    switch (n.type) {
      case 'separator':
        out.push({ separator: true });
        break;
      case 'submenu': {
        const items = buildContextMenu(n.items, entries, on);
        if (items.length) out.push({ label: n.title, submenu: items });
        break;
      }
      case 'command': {
        const e = entries.get(n.key);
        if (e) out.push({ label: e.name, command: e.key });
        break;
      }
      case 'node': {
        const id = n.id;
        out.push({ label: n.name, onSelect: () => on.select(id) });
        break;
      }
      case 'action': {
        const a = n.action;
        out.push({ label: n.title, disabled: !n.enabled, onSelect: () => on.action(a) });
        break;
      }
    }
  }
  return tidy(out);
}
