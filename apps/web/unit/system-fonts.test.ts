// Ответы на запросы системных шрифтов (подсистема 3): любой сбой получения байтов
// — `loadFont` без байтов, чтобы ядро пометило шрифт несостоявшимся и не ждало вечно.

import { expect, test } from '@playwright/test';
import type { CoreEvent } from '../src/wasm/ferrum_wasm';
import { MAX_FONT_BYTES } from '../src/core/limits';
import { installSystemFonts, type SystemFonts } from '../src/ui/system-fonts';

type Sent = { type: string; family?: string; file?: string; bytes?: Uint8Array };

function setup(read: (file: string) => Promise<ArrayBuffer>) {
  let listener: (e: CoreEvent) => void = () => {};
  const sent: Sent[] = [];
  const ed = {
    core: { on: (l: (e: CoreEvent) => void) => ((listener = l), () => {}) },
    store: { setState: () => {}, toast: () => {} },
    send: (cmd: Sent) => sent.push(cmd),
  };
  const src: SystemFonts = { supported: false, granted: async () => false, list: async () => [], read };
  installSystemFonts(ed as unknown as Parameters<typeof installSystemFonts>[0], src);
  const request = (files: string[]) =>
    listener({ type: 'fontRequest', family: 'Helvetica', source: 'system', files } as unknown as CoreEvent);
  return { sent, request };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

test('байты прочитаны — loadFont с байтами', async () => {
  const { sent, request } = setup(async () => new ArrayBuffer(8));
  request(['1']);
  await settle();
  expect(sent).toHaveLength(1);
  expect(sent[0]).toMatchObject({ type: 'loadFont', family: 'Helvetica', file: '1' });
  expect(sent[0].bytes?.byteLength).toBe(8);
});

test('сбой чтения и файл больше предела — loadFont без байтов', async () => {
  const origWarn = console.warn;
  console.warn = () => {};
  try {
    const { sent, request } = setup(async (file) => {
      if (file === 'big') return new ArrayBuffer(MAX_FONT_BYTES + 1);
      throw new Error('Файл шрифта больше 64 МБ');
    });
    request(['big', 'broken']);
    await settle();
    expect(sent).toHaveLength(2);
    for (const cmd of sent) {
      expect(cmd.type).toBe('loadFont');
      expect('bytes' in cmd).toBe(false);
    }
    expect(sent.map((c) => c.file).sort()).toEqual(['big', 'broken']);
  } finally {
    console.warn = origWarn;
  }
});
