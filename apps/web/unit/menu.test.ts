// Меню (спека подсистемы 2, раздел 4): положение у якоря и отражение подменю у
// края окна, переход по первой букве, состояние команд из событий ядра.

import { expect, test } from '@playwright/test';
import { EDGE, placeMenu, typeAhead } from '../src/ui/menu-logic';
import { createUiStore } from '../src/ui/store';

const view = { width: 1000, height: 600 };
const size = { width: 200, height: 300 };

test('корневое меню: под якорем, у нижнего края — над ним, у правого — сдвиг', () => {
  const a = { left: 10, right: 40, top: 10, bottom: 34 };
  expect(placeMenu(a, size, view)).toEqual({ x: 10, y: 38 });
  expect(placeMenu({ left: 10, right: 40, top: 500, bottom: 524 }, size, view)).toEqual({ x: 10, y: 196 });
  expect(placeMenu({ left: 950, right: 990, top: 10, bottom: 34 }, size, view).x).toBe(1000 - EDGE - 200);
  expect(placeMenu({ left: 10, right: 40, top: 400, bottom: 424 }, size, view, { above: true }).y).toBe(96);
});

test('подменю: справа от пункта, у правого края — слева, у нижнего — поднимается', () => {
  const item = { left: 100, right: 300, top: 50, bottom: 74 };
  expect(placeMenu(item, size, view, { submenu: true })).toEqual({ x: 300, y: 46 });
  const atRight = { left: 700, right: 900, top: 50, bottom: 74 };
  expect(placeMenu(atRight, size, view, { submenu: true })).toEqual({ x: 500, y: 46 });
  const low = { left: 100, right: 300, top: 500, bottom: 524 };
  expect(placeMenu(low, size, view, { submenu: true }).y).toBe(600 - EDGE - 300);
  // Не влезает ни справа, ни слева — прижато к краю окна.
  const narrow = { width: 300, height: 600 };
  expect(placeMenu({ left: 60, right: 260, top: 10, bottom: 34 }, size, narrow, { submenu: true }).x).toBe(300 - EDGE - 200);
});

test('переход по первой букве: по кругу, без учёта регистра и «ё»', () => {
  const labels = ['Файл', 'Правка', 'Вид', 'Объект', 'Текст', 'Вектор', 'Ёлка'];
  expect(typeAhead(labels, -1, 'в')).toBe(2);
  expect(typeAhead(labels, 2, 'В')).toBe(5);
  expect(typeAhead(labels, 5, 'в')).toBe(2);
  expect(typeAhead(labels, 0, 'е')).toBe(6);
  expect(typeAhead(labels, 0, 'ж')).toBe(-1);
  expect(typeAhead(labels, 0, ' ')).toBe(-1);
});

test('состояние команд: сначала все, потом только изменившиеся', () => {
  const store = createUiStore();
  store.apply({
    type: 'commandStates',
    changed: [
      { key: 'edit.delete', enabled: false },
      { key: 'view.pixel_grid', enabled: true, checked: true },
    ],
  });
  store.apply({ type: 'commandStates', changed: [{ key: 'edit.delete', enabled: true }] });
  expect(store.state.commands['edit.delete']).toEqual({ enabled: true, checked: undefined });
  expect(store.state.commands['view.pixel_grid']).toEqual({ enabled: true, checked: true });
});
