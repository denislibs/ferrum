// «Копировать как PNG / SVG» без асинхронного буфера (нет `ClipboardItem` или
// `navigator.clipboard` — старый движок, небезопасный контекст) и с отказом: исключений
// нет, SVG уходит обычным текстом через `execCommand`, итог всегда виден уведомлением.

import { expect, test } from '@playwright/test';
import {
  copyPng,
  copySvg,
  PNG_COPIED,
  PNG_FAILED,
  SVG_COPIED,
  SVG_FAILED,
  type ClipboardSink,
} from '../src/ui/clipboard-write';

const g = globalThis as Record<string, unknown>;
const saved = { item: Object.getOwnPropertyDescriptor(g, 'ClipboardItem'), nav: Object.getOwnPropertyDescriptor(g, 'navigator') };

function setGlobal(name: string, value: unknown) {
  Object.defineProperty(g, name, { value, configurable: true, writable: true });
}

test.afterEach(() => {
  for (const [name, d] of [
    ['ClipboardItem', saved.item],
    ['navigator', saved.nav],
  ] as const) {
    if (d) Object.defineProperty(g, name, d);
    else delete g[name];
  }
});

function sink(textOk = true) {
  const log = { toasts: [] as [string, string | undefined][], texts: [] as string[] };
  const s: ClipboardSink = {
    toast: (m, kind) => void log.toasts.push([m, kind]),
    writeText: (t) => {
      log.texts.push(t);
      return textOk;
    },
  };
  return { s, log };
}

const SVG = '<svg width="1" height="1">ё</svg>';
const bytes = () => new TextEncoder().encode(SVG);

/** Асинхронный буфер: запоминает записанные форматы; `fail` — отказ записи. */
function fakeAsync(fail: boolean, svg: boolean) {
  const written: Record<string, Blob>[] = [];
  setGlobal(
    'ClipboardItem',
    class {
      static supports = (t: string) => svg && t === 'image/svg+xml';
      constructor(public items: Record<string, Blob>) {}
    },
  );
  setGlobal('navigator', {
    clipboard: {
      write: async (list: { items: Record<string, Blob> }[]) => {
        if (fail) throw new Error('NotAllowedError');
        written.push(list[0].items);
      },
    },
  });
  return written;
}

test('нет ClipboardItem и navigator.clipboard: SVG — обычным текстом, без исключений', async () => {
  setGlobal('ClipboardItem', undefined);
  setGlobal('navigator', undefined);
  const ok = sink(true);
  await copySvg(bytes(), ok.s);
  expect(ok.log.texts).toEqual([SVG]);
  expect(ok.log.toasts).toEqual([[SVG_COPIED, undefined]]);
  // execCommand тоже отказал — ошибка по-русски.
  const no = sink(false);
  await copySvg(bytes(), no.s);
  expect(no.log.toasts).toEqual([[SVG_FAILED, 'error']]);
  // PNG обычным текстом не скопировать — ошибка, а не исключение.
  const png = sink();
  await copyPng(new Uint8Array([137, 80]), png.s);
  expect(png.log.toasts).toEqual([[PNG_FAILED, 'error']]);
  expect(png.log.texts).toEqual([]);
});

test('ClipboardItem есть, а navigator.clipboard нет (небезопасный контекст)', async () => {
  fakeAsync(false, true);
  setGlobal('navigator', {});
  const s = sink();
  await copySvg(bytes(), s.s);
  expect(s.log.texts).toEqual([SVG]);
  expect(s.log.toasts).toEqual([[SVG_COPIED, undefined]]);
  const png = sink();
  await copyPng(new Uint8Array([1]), png.s);
  expect(png.log.toasts).toEqual([[PNG_FAILED, 'error']]);
});

test('асинхронный буфер: SVG — байтами в text/plain и, где умеет, image/svg+xml', async () => {
  const written = fakeAsync(false, true);
  const s = sink();
  await copySvg(bytes(), s.s);
  expect(Object.keys(written[0]).sort()).toEqual(['image/svg+xml', 'text/plain']);
  expect(await written[0]['text/plain'].text()).toBe(SVG);
  expect(await written[0]['image/svg+xml'].text()).toBe(SVG);
  expect(s.log.texts).toEqual([]);
  expect(s.log.toasts).toEqual([[SVG_COPIED, undefined]]);
  // Без поддержки image/svg+xml — только текст.
  const plain = fakeAsync(false, false);
  await copySvg(bytes(), sink().s);
  expect(Object.keys(plain[0])).toEqual(['text/plain']);
  const png = sink();
  await copyPng(new Uint8Array([1]), png.s);
  expect(png.log.toasts).toEqual([[PNG_COPIED, undefined]]);
});

test('отказ асинхронного буфера: SVG — запасным путём, PNG — ошибка', async () => {
  fakeAsync(true, true);
  const s = sink();
  await copySvg(bytes(), s.s);
  expect(s.log.texts).toEqual([SVG]);
  expect(s.log.toasts).toEqual([[SVG_COPIED, undefined]]);
  const png = sink();
  await copyPng(new Uint8Array([1]), png.s);
  expect(png.log.toasts).toEqual([[PNG_FAILED, 'error']]);
});
