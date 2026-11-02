// Контекстное меню: пункты из дерева ядра — команды с названиями из таблицы,
// «Выделить слой» и действия над целью.

import { expect, test } from '@playwright/test';
import type { KeymapEntry, MenuActionDto, MenuItemDto } from '../src/wasm/ferrum_wasm';
import { buildContextMenu } from '../src/ui/context-menu';
import { entriesByKey } from '../src/ui/main-menu';
import type { MenuItem } from '../src/ui/menu';

const entry = (key: string, name: string): KeymapEntry => ({
  key,
  name,
  group: key.split('.')[0],
  keywords: [],
  toggle: false,
  bindings: [],
  labels: [],
  defaults: [],
  overridden: false,
});

const entries = entriesByKey([entry('edit.copy', 'Копировать'), entry('edit.delete', 'Удалить')]);
const c = (key: string): MenuItemDto => ({ type: 'command', key });
const sep: MenuItemDto = { type: 'separator' };

test('команды, узлы «Выделить слой» и действия; пустое подменю и лишние разделители убраны', () => {
  const selected: string[] = [];
  const actions: MenuActionDto[] = [];
  const tree: MenuItemDto[] = [
    c('edit.copy'),
    sep,
    { type: 'submenu', title: 'Выделить слой', items: [{ type: 'node', id: '1@2', name: 'Прямоугольник' }] },
    sep,
    { type: 'submenu', title: 'Пусто', items: [c('ui.unknown')] },
    sep,
    { type: 'action', action: 'renameLayer', title: 'Переименовать', enabled: false },
    { type: 'action', action: 'expandAll', title: 'Развернуть всё', enabled: true },
    sep,
    c('edit.delete'),
  ];
  const menu = buildContextMenu(tree, entries, { select: (id) => selected.push(id), action: (a) => actions.push(a) });
  const labels = menu.map((m) => ('label' in m ? m.label : 'separator' in m ? '—' : ''));
  expect(labels).toEqual(['Копировать', '—', 'Выделить слой', '—', 'Переименовать', 'Развернуть всё', '—', 'Удалить']);
  const pick = (i: number) => menu[i] as Extract<MenuItem, { label: string }>;
  expect(pick(0).command).toBe('edit.copy');
  (pick(2).submenu![0] as Extract<MenuItem, { label: string }>).onSelect!();
  expect(selected).toEqual(['1@2']);
  expect(pick(4).disabled).toBe(true);
  expect(pick(5).disabled).toBe(false);
  pick(5).onSelect!();
  expect(actions).toEqual(['expandAll']);
});
