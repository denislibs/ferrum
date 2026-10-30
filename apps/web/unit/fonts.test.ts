// Шрифты на стороне TypeScript (подсистема 3): манифест вшитых, системные шрифты
// Chrome в семейства, адрес файла.

import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { isFontPath, localFamilies, manifestFamilies } from '../src/core/fonts';

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
