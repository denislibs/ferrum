import { expect, test } from '@playwright/test';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkDist, LOCAL_MARKER } from '../scripts/check-dist.mjs';

function dist(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), 'ferrum-dist-'));
  mkdirSync(join(dir, 'assets'));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  return dir;
}
const INDEX = '<title>Ferrum</title>';
const DEBUG = '<title>Ferrum — стенд ядра</title>';

test('автономная сборка: обе страницы и признак режима', () => {
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'standalone')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'standalone')).toContain('debug.html не собран');
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': 'x' }), 'standalone')).toContain(
    'нет автономного режима',
  );
});

test('рабочая сборка: без стенда и кода автономного режима', () => {
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': 'x' }), 'server')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': 'x' }), 'server')).toContain(
    'debug.html в рабочей сборке',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1` }), 'server')).toContain(
    'есть код автономного режима',
  );
});

test('десктоп перед сборкой собирает автономный режим', () => {
  const tauri = JSON.parse(readFileSync('../desktop/src-tauri/tauri.conf.json', 'utf8'));
  expect(tauri.build.beforeBuildCommand).toBe('npm --prefix ../web run build');
  expect(tauri.build.frontendDist).toBe('../../web/dist');
  const web = JSON.parse(readFileSync('package.json', 'utf8'));
  expect(web.scripts.build).toContain('--mode standalone');
  expect(web.scripts['build:server']).not.toContain('standalone');
});
