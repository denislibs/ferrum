// Шрифты на стороне TypeScript (подсистема 3): манифест вшитых, системные шрифты
// Chrome в семейства, адрес файла и начертание замены по умолчанию.

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { defaultStyle, desktopFamilies, isFontPath, localFamilies, manifestFamilies, styleKey } from '../src/core/fonts';
import { MAX_FAMILY_NAME, MAX_FONT_FACES, MAX_FONT_FAMILIES } from '../src/core/limits';

test('манифест сборки читается целиком, мусор пропускается', () => {
  const manifest = JSON.parse(readFileSync(new URL('../public/fonts/manifest.json', import.meta.url), 'utf8'));
  const families = manifestFamilies(manifest);
  expect(families.map((f) => f.family)).toEqual(
    ['Inter', 'JetBrains Mono', 'Montserrat', 'Noto Sans', 'Open Sans', 'PT Sans', 'PT Serif', 'Playfair Display', 'Roboto'],
  );
  const inter = families.find((f) => f.family === 'Inter')!;
  expect(inter.faces.find((f) => f.style === 'Bold')).toMatchObject({ weight: 700, italic: false, file: 'inter/Inter-Variable.ttf' });
  expect(manifestFamilies({ families: [{ family: 'X', faces: [{ style: 'R', file: '../../etc/passwd.ttf' }] }] })).toEqual([]);
  expect(manifestFamilies(null)).toEqual([]);
});

test('путь файла шрифта — только внутри public/fonts', () => {
  expect(isFontPath('inter/Inter-Variable.ttf')).toBe(true);
  for (const bad of ['../x.ttf', '/abs.ttf', 'inter/x.woff2', 'a//b.ttf', 'http://x/y.ttf', 'inter/../x.ttf']) {
    expect(isFontPath(bad), bad).toBe(false);
  }
});

test('системные шрифты Chrome группируются по семействам', () => {
  const fams = localFamilies([
    { family: 'Helvetica Neue', style: 'Regular', postscriptName: 'HelveticaNeue' },
    { family: 'Helvetica Neue', style: 'Bold', postscriptName: 'HelveticaNeue-Bold' },
    { family: 'Helvetica Neue', style: 'Bold', postscriptName: 'HelveticaNeue-Bold2' },
    { family: '', style: 'Regular', postscriptName: 'Nameless' },
  ]);
  expect(fams).toEqual([
    {
      family: 'Helvetica Neue',
      faces: [
        { style: 'Regular', file: 'HelveticaNeue' },
        { style: 'Bold', file: 'HelveticaNeue-Bold' },
      ],
    },
  ]);
});

test('начертание замены: то же, иначе Regular, иначе первое', () => {
  expect(styleKey('Semi Bold Italic')).toBe(styleKey('SemiBold Italic'));
  const roboto = { family: 'Roboto', source: 'bundled' as const, styles: ['Thin', 'Regular', 'SemiBold Italic'] };
  expect(defaultStyle(roboto, 'Semi Bold Italic')).toBe('SemiBold Italic');
  expect(defaultStyle(roboto, 'Wide')).toBe('Regular');
  expect(defaultStyle({ ...roboto, styles: ['Thin'] }, 'Wide')).toBe('Thin');
  expect(defaultStyle(undefined, 'Bold')).toBe('');
});

test('системные шрифты десктопа — в пределах ядра, неверное пропускается, а не губит список', () => {
  const face = (style: string, file: string) => ({ style, weight: 400, italic: false, stretch: 1, file, index: 0 });
  const list: unknown[] = [
    { family: 'x'.repeat(MAX_FAMILY_NAME + 1), faces: [face('Regular', '0')] },
    { family: 'Mixed', faces: [face('Regular', '1'), face('', '2'), face('Bold', '../etc'), { style: 'Thin' }, null] },
    { family: 'Many', faces: Array.from({ length: MAX_FONT_FACES + 3 }, (_, i) => face(`S${i}`, String(i))) },
    { family: 'Empty', faces: [] },
    'мусор',
    ...Array.from({ length: MAX_FONT_FAMILIES }, (_, i) => ({ family: `F${i}`, faces: [face('Regular', String(i))] })),
  ];
  const { families, dropped } = desktopFamilies(list);
  expect(families[0]).toEqual({ family: 'Mixed', faces: [face('Regular', '1')] });
  expect(families[1].faces).toHaveLength(MAX_FONT_FACES);
  expect(families.length).toBeLessThanOrEqual(MAX_FONT_FAMILIES);
  expect(families.some((f) => f.family === 'Empty')).toBe(false);
  // Длинное имя, 4 начертания Mixed, 3 лишних у Many, пустое семейство, мусор, 5 семейств за пределом.
  expect(dropped).toBe(1 + 4 + 3 + 1 + 1 + 5);
  expect(desktopFamilies(null)).toEqual({ families: [], dropped: 0 });
});
