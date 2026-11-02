// Главное меню: пункты из дерева ядра и поиск по командам (подстрока, синонимы,
// раскладка ЙЦУКЕН ↔ QWERTY).

import { expect, test } from '@playwright/test';
import type { KeymapEntry, MenuItemDto } from '../src/wasm/ferrum_wasm';
import { buildMainMenu, entriesByKey, layoutVariants, MAX_RESULTS, searchMenu } from '../src/ui/main-menu';

const entry = (key: string, name: string, keywords: string[] = []): KeymapEntry => ({
  key,
  name,
  group: key.split('.')[0],
  keywords,
  toggle: false,
  bindings: [],
  labels: [],
  defaults: [],
  overridden: false,
});

const entries = entriesByKey([
  entry('ui.new_file', 'Новый файл', ['создать', 'new file']),
  entry('ui.open_disk', 'Открыть с диска…', ['open from disk']),
  entry('ui.save_disk', 'Сохранить на диск…', ['save to disk']),
  entry('file.save', 'Сохранить', ['save']),
  entry('edit.duplicate', 'Дублировать', ['дубль', 'копия', 'duplicate']),
  entry('arrange.front', 'На передний план', ['порядок', 'bring to front', 'order']),
  entry('edit.copy_png', 'Копировать как PNG', ['copy as png']),
]);

const c = (key: string): MenuItemDto => ({ type: 'command', key });
const sep: MenuItemDto = { type: 'separator' };
const tree: MenuItemDto[] = [
  { type: 'submenu', title: 'Файл', items: [c('ui.new_file'), c('ui.open_disk'), sep, c('file.save'), c('ui.save_disk')] },
  { type: 'submenu', title: 'Правка', items: [c('edit.duplicate'), { type: 'submenu', title: 'Копировать как', items: [c('edit.copy_png')] }] },
  { type: 'submenu', title: 'Расставить', items: [c('arrange.front')] },
  { type: 'submenu', title: 'Пусто', items: [c('ui.unknown')] },
];

test('пункты из дерева: названия из таблицы, недоступное на платформе — пропущено', () => {
  const menu = buildMainMenu(tree, entries, new Set(['ui.open_disk', 'ui.save_disk']));
  expect(menu.map((m) => ('label' in m ? m.label : '—'))).toEqual(['Файл', 'Правка', 'Расставить']);
  const file = menu[0] as { submenu: { label?: string; command?: string; separator?: true }[] };
  expect(file.submenu.map((m) => m.command ?? '—')).toEqual(['ui.new_file', '—', 'file.save']);
  const edit = menu[1] as { submenu: { label: string; submenu?: unknown[] }[] };
  expect(edit.submenu[1].label).toBe('Копировать как');
});

test('поиск: подстрока названия и синонима, сначала начало названия', () => {
  const keys = (q: string) => searchMenu(tree, entries, q).map((m) => ('command' in m ? m.command : ''));
  expect(keys('сохр')).toEqual(['file.save', 'ui.save_disk']);
  expect(keys('копи')).toEqual(['edit.copy_png', 'edit.duplicate']);
  expect(keys('DUPLICATE')).toEqual(['edit.duplicate']);
  expect(keys('  ')).toEqual([]);
  expect(keys('нет такого')).toEqual([]);
});

test('поиск не зависит от раскладки: «щквук» — «order», «le,km» — «дубль»', () => {
  expect(layoutVariants('щквук')).toContain('order');
  expect(layoutVariants('le,km')).toContain('дубль');
  expect(searchMenu(tree, entries, 'щквук').map((m) => ('command' in m ? m.command : ''))).toEqual(['arrange.front']);
  expect(searchMenu(tree, entries, 'le,km').map((m) => ('command' in m ? m.command : ''))).toEqual(['edit.duplicate']);
  // «ё» и «е» — одно.
  expect(layoutVariants('ЁЛКА')).toContain('елка');
});

test('не больше 50 результатов', () => {
  const many = Array.from({ length: 80 }, (_, i) => entry(`x.c${i}`, `Команда ${i}`));
  const t: MenuItemDto[] = many.map((e) => c(e.key));
  expect(searchMenu(t, entriesByKey(many), 'команда')).toHaveLength(MAX_RESULTS);
});
