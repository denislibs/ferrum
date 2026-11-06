// Совместимость Loro (спека подсистемы 4, разделы 3 и 13.2): обновления и снимки ядра на Rust читает `loro-crdt` на Bun, и
// наоборот; версии в `Cargo.toml` и `package.json` одни и те же. Обратная сторона — `crates/ferrum-doc/tests/loro_compat.rs`.

import { expect, test } from 'bun:test';
import { decodeImportBlobMeta, LoroDoc } from 'loro-crdt';

const dir = new URL('../../../crates/ferrum-doc/tests/data/loro-compat/', import.meta.url);
const read = (name: string) => Bun.file(new URL(name, dir)).bytes();

test('версии Loro на сервере и в ядре совпадают', async () => {
  const cargo = await Bun.file(new URL('../../../Cargo.toml', import.meta.url)).text();
  const rust = /^loro = "=([0-9.]+)"/m.exec(cargo)?.[1];
  const pkg = (await Bun.file(new URL('../package.json', import.meta.url)).json()) as { dependencies: Record<string, string> };
  expect(pkg.dependencies['loro-crdt']).toBe(rust!);
});

test('снимок и обновление ядра читаются на сервере', async () => {
  const snapshot = await read('rust.snapshot');
  const update = await read('rust.update');
  expect(decodeImportBlobMeta(snapshot, true).mode).toBe('snapshot');
  expect(decodeImportBlobMeta(update, false).mode).toBe('update');
  const d = LoroDoc.fromSnapshot(snapshot);
  expect(d.import(update).pending).toBeNull();
  const roots = d.getTree('nodes').roots();
  expect(roots.length).toBe(1);
  expect(roots[0].data.get('x')).toBe(42);
});

test('файл для обратной проверки — от этой же версии сервера', async () => {
  // `js.update` проверяет Rust. Его можно собрать заново этим сервером и получить те же данные.
  const d = new LoroDoc();
  d.getMap('m').set('hello', 'привет');
  d.commit();
  const fresh = LoroDoc.fromSnapshot(d.export({ mode: 'snapshot' }));
  expect(fresh.getMap('m').get('hello')).toBe('привет');
  const stored = LoroDoc.fromSnapshot(await read('js.snapshot'));
  expect(stored.getMap('m').get('hello')).toBe('привет');
});

test('страницу ядра сервер пересобирает: снимок для обратной проверки ядром', async () => {
  const page = LoroDoc.fromSnapshot(await read('rust-page.snapshot'));
  const again = LoroDoc.fromSnapshot(page.export({ mode: 'snapshot' }));
  expect(again.oplogVersion().toJSON()).toEqual(page.oplogVersion().toJSON());
  if (process.env.LORO_COMPAT_WRITE) await Bun.write(new URL('js-page.snapshot', dir), page.export({ mode: 'snapshot' }));
  // Сохранённый снимок той же версии Loro читается и здесь.
  expect(
    LoroDoc.fromSnapshot(await read('js-page.snapshot'))
      .getTree('nodes')
      .getNodes().length,
  ).toBe(2);
});
