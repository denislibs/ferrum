// Хранилище десктопа со стороны интерфейса (этап 6): вызовы IPC Tauri — без окна,
// подменой `window.__TAURI_INTERNALS__`. Запись — сырым телом с параметрами в
// заголовке, превью — отдельно (или не трогается), чтение — байты, сеанс журнала и
// пачки журнала по порядку, пустой ответ — новый файл без журнала, отказ Rust — ошибка
// с тем же текстом.

import { expect, test } from '@playwright/test';
import { isTauri, TauriFileStore } from '../src/ui/tauri';

const ID = '0f8e2a06-1111-4222-8333-444455556666';

function fakeIpc(replies: Record<string, (args: unknown) => unknown>) {
  const calls: { cmd: string; args: unknown; headers?: Record<string, string> }[] = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args: unknown, opts?: { headers?: Record<string, string> }) => {
        calls.push({ cmd, args, headers: opts?.headers });
        const r = replies[cmd];
        if (!r) throw `нет команды ${cmd}`;
        return r(args);
      },
    },
  };
  return calls;
}

test.afterEach(() => {
  delete (globalThis as unknown as { window?: unknown }).window;
});

test('isTauri — только с IPC окна', () => {
  expect(isTauri()).toBe(false);
  fakeIpc({});
  expect(isTauri()).toBe(true);
});

test('запись — сырым телом с параметрами в заголовке; превью отдельно или не трогается', async () => {
  const calls = fakeIpc({
    files_write: () => ({ id: ID, version: 2 }),
    files_write_thumb: () => ({ id: ID, version: 2, thumbVersion: 1 }),
  });
  const s = new TauriFileStore();
  const m = await s.write(ID, new Uint8Array([1, 2, 3]).buffer, new Uint8Array([9]).buffer, 1, { session: 5, upTo: 2 });
  expect(m).toEqual({ id: ID, version: 2, thumbVersion: 1 });
  expect(calls.map((c) => c.cmd)).toEqual(['files_write', 'files_write_thumb']);
  expect(calls[0].args).toEqual(new Uint8Array([1, 2, 3]));
  expect(calls[0].headers).toEqual({ 'x-ferrum': JSON.stringify({ id: ID, base: 1, session: 5, upTo: 2 }) });
  expect(calls[1].args).toEqual(new Uint8Array([9]));
  calls.length = 0;
  await s.write(ID, new ArrayBuffer(4), 'keep', 2, null);
  expect(calls.map((c) => c.cmd)).toEqual(['files_write']);
  expect(calls[0].headers).toEqual({ 'x-ferrum': JSON.stringify({ id: ID, base: 2 }) });
  calls.length = 0;
  await s.write(ID, new ArrayBuffer(4), null, 3, null);
  expect(calls[1]).toMatchObject({ cmd: 'files_write_thumb', args: new Uint8Array(0) });
});

test('чтение — байты и пачки журнала по порядку; пустой ответ — новый файл без журнала', async () => {
  const meta = { id: ID, name: 'Макет', version: 1 };
  let bytes = new Uint8Array([7, 7]).buffer;
  let session: number | null = 4;
  const calls = fakeIpc({
    files_read: () => bytes,
    files_get: () => meta,
    files_journal_session: () => session,
    files_journal_names: () => ['a', 'b'],
    files_journal_read: (a) => new Uint8Array([(a as { name: string }).name === 'a' ? 1 : 2]).buffer,
  });
  const s = new TauriFileStore();
  const r = await s.read(ID);
  expect(r.meta).toEqual(meta);
  expect(new Uint8Array(r.bytes!)).toEqual(new Uint8Array([7, 7]));
  expect(r.journal.map((b) => new Uint8Array(b)[0])).toEqual([1, 2]);
  expect(r.session).toBe(4);
  bytes = new ArrayBuffer(0);
  calls.length = 0;
  // Сеанс — наибольший среди пачек и без байтов файла (как у веб-хранилища).
  const empty = await s.read(ID);
  expect(empty.bytes).toBeNull();
  expect(empty.journal).toEqual([]);
  expect(empty.session).toBe(4);
  expect(calls.some((c) => c.cmd === 'files_journal_names')).toBe(false);
  session = null;
  expect((await s.read(ID)).session).toBeNull();
});

test('отказ Rust — ошибка с тем же текстом; список узнаёт о своих правках', async () => {
  fakeIpc({
    files_rename: () => {
      throw 'Файл не найден';
    },
    files_trash: () => null,
  });
  const s = new TauriFileStore();
  await expect(s.rename(ID, 'x')).rejects.toThrow('Файл не найден');
  let changed = 0;
  s.onChange(() => changed++);
  await s.trash(ID);
  expect(changed).toBe(1);
});
