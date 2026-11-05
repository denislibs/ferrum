// Настоящий клиент файлов против стенда и настоящих пределов частоты: файл с 1200 картинками открывается, 600 новых
// импортируются. Числа важны: при 1200 запросов чтения в минуту на человека такой файл не открывался.

import { expect, test } from '@playwright/test';
import { containerHeader, imageSectionHeader, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { blake3 } from '../../server/node_modules/@noble/hashes/blake3.js';
import { bytesToHex } from '../../server/node_modules/@noble/hashes/utils.js';
import { ServerFiles, type JournalStore } from '../src/core/server-files';
import { testMode } from '../e2e/helpers';
import { STAND_URL } from '../playwright.server.config';
import { newEmail, resetStand, signIn } from './stand';

test.beforeEach(resetStand);

interface Image {
  hash: string;
  bytes: Uint8Array;
}

/** Свои картинки у каждого прогона: стенд живёт дольше одного. */
const run = crypto.getRandomValues(new Uint8Array(16));
const imageOf = (i: number): Image => {
  const bytes = new Uint8Array(32);
  new DataView(bytes.buffer).setUint32(0, i);
  bytes.set(run, 4);
  return { hash: bytesToHex(blake3(bytes)), bytes };
};

function section(kind: number, id: string, data: Uint8Array): Uint8Array {
  const name = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + name.length + 40 + data.length);
  const view = new DataView(out.buffer);
  out[0] = kind;
  view.setUint32(1, name.length, true);
  out.set(name, 5);
  view.setBigUint64(5 + name.length, BigInt(data.length), true);
  out.set(data, 5 + name.length + 40);
  return out;
}

/** Файл как из ядра: корень, ссылки страницы и секции самих картинок. */
function fileWith(images: Image[]): ArrayBuffer {
  const parts: Uint8Array[] = [
    section(1, '', new Uint8Array(16)),
    section(SECTION_PAGE_REFS, '1@1', new TextEncoder().encode(images.map((i) => i.hash).join('\n'))),
  ];
  for (const image of images) parts.push(imageSectionHeader(image.hash, image.bytes.length), image.bytes);
  const head = containerHeader(2 + images.length);
  const out = new Uint8Array(head.length + parts.reduce((n, p) => n + p.length, 0));
  out.set(head);
  let at = head.length;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out.buffer as ArrayBuffer;
}

const journal: JournalStore = {
  append: async () => {},
  read: async () => ({ entries: [], session: null, tooLarge: false }),
  cut: async () => {},
  cutExact: async () => {},
  drop: async () => {},
};

/** Каждое открытие — своя вкладка: кэша известных картинок нет. Считает ответы 429. */
function client(cookie: string) {
  const counter = { limited: 0 };
  const http = (async (input: string | URL | Request, init: RequestInit = {}) => {
    const headers = new Headers(init.headers);
    headers.set('origin', STAND_URL);
    headers.set('cookie', cookie);
    const res = await fetch(`${STAND_URL}${String(input)}`, { ...init, headers });
    if (res.status === 429) counter.limited++;
    return res;
  }) as typeof fetch;
  const clock = {
    now: () => performance.now(),
    setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
    clearTimeout: (t: unknown) => clearTimeout(t as ReturnType<typeof setTimeout>),
  };
  return { files: new ServerFiles(http, journal, { changed: () => {}, access: () => {} }, clock), counter };
}

test('настоящие числа: 600 новых картинок импортируются, файл с 1200 пишется и открывается без единого 429', async ({ page }, info) => {
  test.skip(info.project.name !== 'server', 'клиент здесь в Node, браузер нужен только для входа');
  await testMode(page);
  await page.goto('/');
  await signIn(page, newEmail(info.project.name));
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const cookie = (await page.context().cookies()).map((c) => `${c.name}=${c.value}`).join('; ');

  const first = Array.from({ length: 600 }, (_, i) => imageOf(i));
  const all = [...first, ...Array.from({ length: 600 }, (_, i) => imageOf(600 + i))];
  const writer = client(cookie);
  const made = await writer.files.create('Много картинок', fileWith(first), null);
  expect(made.version).toBe(1);
  const second = await writer.files.write(made.id, fileWith(all), 'keep', 1, null);
  expect(second.version).toBe(2);
  expect(writer.counter.limited).toBe(0);

  const reader = client(cookie);
  const opened = await reader.files.read(made.id);
  expect(reader.counter.limited).toBe(0);
  expect(opened.meta.version).toBe(2);
  // Все картинки на месте: собранный файл того же размера, что исходный.
  expect(opened.bytes!.byteLength).toBe(fileWith(all).byteLength);
});
