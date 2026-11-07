import { expect, test } from '@playwright/test';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  AUTH_MARKER,
  checkDist,
  COMMENTS_MARKER,
  FILES_MARKER,
  LOCAL_MARKER,
  TAURI_STORE_MARKER,
  TEST_STORE_MARKER,
} from '../scripts/check-dist.mjs';

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

test('автономная сборка: без кода файлов на сервере', () => {
  const files = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${FILES_MARKER}')` };
  expect(checkDist(dist(files), 'standalone')).toContain('код файлов на сервере');
});

test('автономная сборка: без комментариев — ни кода, ни стилей', () => {
  const base = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;h('fe-comments-host')` };
  expect(checkDist(dist({ ...base, 'assets/main.css': '.fe-following{}' }), 'standalone')).toBeNull();
  expect(checkDist(dist({ ...base, 'assets/main.css': `.${COMMENTS_MARKER}{position:absolute}` }), 'standalone')).toContain(
    'есть комментарии',
  );
  const js = { ...base, 'assets/main.js': `x.${LOCAL_MARKER}=1;c('${COMMENTS_MARKER}')` };
  expect(checkDist(dist(js), 'standalone')).toContain('есть комментарии');
});

test('автономная сборка: без кода входа', () => {
  const files = { 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': `x.${LOCAL_MARKER}=1;f('${AUTH_MARKER}')` };
  expect(checkDist(dist(files), 'standalone')).toContain('есть код входа');
});

test('рабочая сборка: без стенда и кода автономного режима', () => {
  const server = `f('${AUTH_MARKER}session');g('${FILES_MARKER}');c('${COMMENTS_MARKER}')`;
  const css = `.${COMMENTS_MARKER}{}`;
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': server, 'assets/main.css': css }), 'server')).toBeNull();
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': server }), 'server')).toContain('нет комментариев');
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `f('${AUTH_MARKER}session')` }), 'server')).toContain('нет кода файлов');
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `${server};h('${TAURI_STORE_MARKER}')` }), 'server')).toContain(
    'хранилище десктопа',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `${server};w.${TEST_STORE_MARKER}=1` }), 'server')).toContain(
    'тестовый доступ',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `g('${FILES_MARKER}')` }), 'server')).toContain('нет кода входа');
  expect(checkDist(dist({ 'index.html': INDEX, 'debug.html': DEBUG, 'assets/main.js': server }), 'server')).toContain(
    'debug.html в рабочей сборке',
  );
  expect(checkDist(dist({ 'index.html': INDEX, 'assets/main.js': `x.${LOCAL_MARKER}=1;${server}` }), 'server')).toContain(
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
