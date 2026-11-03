// Список «Быстрых действий»: недавние, команды, слои и страницы с заголовками;
// не больше 50 строк; недавние — в настройках строкой ключей.

import { expect, test } from '@playwright/test';
import type { FoundLayerDto, KeymapEntry } from '../src/wasm/ferrum_wasm';
import {
  bindingsToCommands,
  clipQuery,
  COMMANDS_MAX,
  MAX_QUERY_BYTES,
  PAGES_MAX,
  PALETTE_ROWS,
  paletteRows,
  parseRecent,
  pushRecent,
  rankCommands,
  RECENT_MAX,
  type PaletteRow,
} from '../src/ui/palette';

const entry = (key: string, name: string, keywords: string[] = [], group = key.split('.')[0]): KeymapEntry => ({
  key,
  name,
  group,
  keywords,
  toggle: false,
  bindings: [],
  labels: [],
  defaults: [],
  overridden: false,
});

const commands = [
  entry('edit.undo', 'Отменить', ['undo']),
  entry('edit.duplicate', 'Дублировать', ['дубль', 'копия', 'duplicate']),
  entry('edit.delete', 'Удалить', ['стереть', 'delete']),
  entry('arrange.front', 'На передний план', ['порядок', 'bring to front', 'order']),
  entry('arrange.back', 'На задний план', ['порядок', 'send to back', 'order']),
  entry('tool.hand_hold', 'Рука, пока клавиша нажата', ['hand']),
  entry('nudge.left', 'Сдвинуть влево', ['nudge']),
  entry('ui.quick_actions', 'Быстрые действия…', ['quick actions']),
  entry('ui.open_disk', 'Открыть с диска…', ['open from disk']),
];

const rows = (over: Partial<Parameters<typeof paletteRows>[0]> = {}) =>
  paletteRows({ query: '', commands, hidden: new Set(), recent: [], layers: [], pages: [], ...over });
const show = (r: PaletteRow[]) => r.map((x) => (x.kind === 'heading' ? `# ${x.title}` : x.kind === 'command' ? x.key : `${x.kind}:${x.name}`));

test('пустой запрос: недавние, затем остальные команды; без удержания, сдвигов и самой палитры', () => {
  expect(show(rows({ recent: ['edit.delete', 'ui.unknown', 'edit.undo'], hidden: new Set(['ui.open_disk']) }))).toEqual([
    '# Недавние',
    'edit.delete',
    'edit.undo',
    '# Команды',
    'edit.duplicate',
    'arrange.front',
    'arrange.back',
  ]);
  // Нет недавних — нет и заголовка.
  expect(show(rows())[0]).toBe('# Команды');
});

test('запрос: команды, слои, страницы с заголовками; раскладка и недавность', () => {
  const layers: FoundLayerDto[] = [{ id: '1@2', name: 'Порядок карточек', kind: 'frame' }];
  const r = rows({ query: 'порядок', layers, pages: [{ id: '0@1', name: 'Порядок' }, { id: '0@2', name: 'Обложка' }] });
  expect(show(r)).toEqual(['# Команды', 'arrange.front', 'arrange.back', '# Слои', 'layer:Порядок карточек', '# Страницы', 'page:Порядок']);
  // «щквук» — «order» в русской раскладке; недавняя команда — первой.
  expect(show(rows({ query: 'щквук', recent: ['arrange.back'] }))).toEqual(['# Команды', 'arrange.back', 'arrange.front']);
  // Ничего не совпало — пусто.
  expect(rows({ query: 'нет такого' })).toEqual([]);
});

test('не больше 50 строк: команды до 20, страницы до 10, остальное — слоям', () => {
  const many = Array.from({ length: 40 }, (_, i) => entry(`x.c${i}`, `Слой команда ${i}`));
  const layers: FoundLayerDto[] = Array.from({ length: 50 }, (_, i) => ({ id: `${i}@1`, name: `Слой ${i}`, kind: 'rect' }));
  const pages = Array.from({ length: 15 }, (_, i) => ({ id: `${i}@9`, name: `Слой-страница ${i}` }));
  const r = rows({ query: 'слой', commands: many, layers, pages });
  const count = (k: PaletteRow['kind']) => r.filter((x) => x.kind === k).length;
  expect(count('command')).toBe(COMMANDS_MAX);
  expect(count('page')).toBe(PAGES_MAX);
  expect(count('layer')).toBe(PALETTE_ROWS - COMMANDS_MAX - PAGES_MAX);
  expect(r.length - count('heading')).toBe(PALETTE_ROWS);
  // Пустой запрос и сотня команд — тоже 50.
  const hundred = Array.from({ length: 100 }, (_, i) => entry(`x.c${i}`, `Команда ${i}`));
  expect(rows({ commands: hundred }).filter((x) => x.kind === 'command')).toHaveLength(PALETTE_ROWS);
});

test('поиск команд: название выше синонима, равный вес — порядок таблицы', () => {
  const keys = (q: string, recent: string[] = []) => rankCommands(commands, q, recent).map((e) => e.key);
  // «дубл» — название «Дублировать» и синоним «дубль»: одна команда, не две.
  expect(keys('дубл')).toEqual(['edit.duplicate']);
  // «копия» только у синонима — команда находится.
  expect(keys('копия')).toEqual(['edit.duplicate']);
  // Подряд в «план» — выше букв вразброс («Рука, пока клавиша нажата»).
  expect(keys('план').slice(0, 2)).toEqual(['arrange.front', 'arrange.back']);
  expect(keys('план', ['arrange.back']).slice(0, 2)).toEqual(['arrange.back', 'arrange.front']);
});

test('недавние: в начало, без повторов, не больше 8; из настроек — только верные ключи', () => {
  let r: string[] = [];
  for (let i = 0; i < 10; i++) r = pushRecent(r, `edit.c${i}`);
  expect(r).toHaveLength(RECENT_MAX);
  expect(r[0]).toBe('edit.c9');
  r = pushRecent(r, 'edit.c5');
  expect(r[0]).toBe('edit.c5');
  expect(r.filter((k) => k === 'edit.c5')).toHaveLength(1);
  expect(pushRecent(r, 'мусор')).toEqual(r);
  expect(parseRecent('edit.undo  edit.undo ui.export <script> x'.padEnd(10))).toEqual(['edit.undo', 'ui.export']);
  expect(parseRecent(Array.from({ length: 20 }, (_, i) => `edit.c${i}`).join(' '))).toHaveLength(RECENT_MAX);
  expect(parseRecent('')).toEqual([]);
});

test('запрос к worker\'у — не длиннее 256 байт, по границе символа', () => {
  expect(clipQuery('кнопка')).toBe('кнопка');
  const long = 'я'.repeat(200);
  const clipped = clipQuery(long);
  expect(new TextEncoder().encode(clipped).length).toBeLessThanOrEqual(MAX_QUERY_BYTES);
  expect(clipped).toBe('я'.repeat(128));
});

test('сочетание → команда: первая в таблице, если занято двумя', () => {
  const list = [
    { ...entry('edit.undo', 'Отменить'), bindings: ['Ctrl+KeyZ'] },
    { ...entry('edit.redo', 'Повторить'), bindings: ['Ctrl+Shift+KeyZ', 'Ctrl+KeyY'] },
    { ...entry('x.same', 'Другая'), bindings: ['Ctrl+KeyZ'] },
  ];
  const m = bindingsToCommands(list);
  expect(m.get('Ctrl+KeyZ')).toBe('edit.undo');
  expect(m.get('Ctrl+KeyY')).toBe('edit.redo');
  expect(m.get('KeyQ')).toBeUndefined();
});
