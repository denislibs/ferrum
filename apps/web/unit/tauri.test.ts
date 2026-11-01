// Хранилище десктопа со стороны интерфейса (этап 6): вызовы IPC Tauri — без окна,
// подменой `window.__TAURI_INTERNALS__`. Запись — сырым телом с параметрами в
// заголовке, превью — отдельно (или не трогается), чтение — байты, сеанс журнала и
// пачки журнала по порядку, пустой ответ — новый файл без журнала, отказ Rust — ошибка
// с тем же текстом.

import { expect, test } from '@playwright/test';
import { asciiJson, isTauri, saveExport, TauriFileStore } from '../src/ui/tauri';

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

test('чтение — одним снимком: метаданные, сеанс и части (файл, затем пачки по порядку); пустое — новый файл без журнала', async () => {
  const meta = { id: ID, name: 'Макет', version: 1 };
  let head = { meta, session: 4 as number | null, ticket: 9, parts: 3, bytes: true, journalDropped: false };
  let parts = [new Uint8Array([7, 7]).buffer, new Uint8Array([1]).buffer, new Uint8Array([2]).buffer];
  const calls = fakeIpc({
    files_read_open: () => head,
    files_read_part: (a) => {
      expect(a).toEqual({ ticket: head.ticket });
      return parts.shift();
    },
  });
  const s = new TauriFileStore();
  const r = await s.read(ID);
  expect(calls[0]).toMatchObject({ cmd: 'files_read_open', args: { id: ID } });
  expect(calls.map((c) => c.cmd)).toEqual(['files_read_open', 'files_read_part', 'files_read_part', 'files_read_part']);
  expect(r.meta).toEqual(meta);
  expect(new Uint8Array(r.bytes!)).toEqual(new Uint8Array([7, 7]));
  expect(r.journal.map((b) => new Uint8Array(b)[0])).toEqual([1, 2]);
  expect(r.session).toBe(4);
  expect(r.journalTooLarge).toBe(false);
  // Сеанс — наибольший среди пачек и без байтов файла (как у веб-хранилища).
  head = { ...head, parts: 0, bytes: false };
  calls.length = 0;
  const empty = await s.read(ID);
  expect(empty.bytes).toBeNull();
  expect(empty.journal).toEqual([]);
  expect(empty.session).toBe(4);
  expect(calls.map((c) => c.cmd)).toEqual(['files_read_open']);
  // Журнал сверх пределов: файл без журнала и признак — уведомит редактор.
  head = { ...head, parts: 1, bytes: true, journalDropped: true, session: null };
  parts = [new Uint8Array([5]).buffer];
  const big = await s.read(ID);
  expect(new Uint8Array(big.bytes!)).toEqual(new Uint8Array([5]));
  expect(big.journal).toEqual([]);
  expect(big.journalTooLarge).toBe(true);
  expect(big.session).toBeNull();
});

test('чтение, запись и журнал — по очереди: чтение не встаёт между частями записи', async () => {
  const log: string[] = [];
  let release!: () => void;
  const held = new Promise<void>((r) => (release = r));
  fakeIpc({
    files_write: async () => {
      log.push('write:start');
      await held;
      log.push('write:end');
      return { id: ID, version: 2 };
    },
    files_write_thumb: () => {
      log.push('thumb');
      return { id: ID, version: 2, thumbVersion: 1 };
    },
    files_journal_append: () => {
      log.push('append');
      return null;
    },
    files_read_open: () => {
      log.push('read');
      return { meta: { id: ID, version: 2 }, session: null, ticket: 1, parts: 0, bytes: false, journalDropped: false };
    },
  });
  const s = new TauriFileStore();
  const w = s.write(ID, new ArrayBuffer(1), new ArrayBuffer(1), 1, { session: 1, upTo: 1 });
  const r = s.read(ID);
  const a = s.appendJournal(ID, { session: 2, batch: 1 }, new ArrayBuffer(1));
  await new Promise((t) => setTimeout(t, 10));
  expect(log).toEqual(['write:start']);
  release();
  await Promise.all([w, r, a]);
  expect(log).toEqual(['write:start', 'write:end', 'thumb', 'read', 'append']);
  // Отказ в очереди не останавливает следующие.
  fakeIpc({
    files_journal_append: () => {
      throw 'нет доступа к файлу или папке';
    },
    files_read_open: () => ({ meta: { id: ID }, session: null, ticket: 1, parts: 0, bytes: false, journalDropped: false }),
  });
  await expect(s.appendJournal(ID, { session: 2, batch: 2 }, new ArrayBuffer(1))).rejects.toThrow('нет доступа к файлу или папке');
  await expect(s.read(ID)).resolves.toMatchObject({ bytes: null });
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

test('отказ превью после записи файла — запись удалась, предупреждение на русском', async () => {
  const calls = fakeIpc({
    files_write: () => ({ id: ID, version: 2 }),
    files_write_thumb: () => {
      throw 'Слишком большой объём данных: 9 МБ, предел — 8 МБ';
    },
  });
  const s = new TauriFileStore();
  let changed = 0;
  s.onChange(() => changed++);
  const m = await s.write(ID, new ArrayBuffer(4), new ArrayBuffer(1), 1, null);
  expect(calls.map((c) => c.cmd)).toEqual(['files_write', 'files_write_thumb']);
  // Поколение — от записи файла: следующая запись не получит ложный CONFLICT.
  expect(m).toEqual({
    id: ID,
    version: 2,
    warning: 'Превью файла не записано: Слишком большой объём данных: 9 МБ, предел — 8 МБ',
  });
  expect(changed).toBe(1);
});

test('отказы самого Tauri (аргументы, состояние) — по-русски', async () => {
  fakeIpc({
    files_get: () => {
      throw 'invalid args `id` for command `files_get`: invalid type: integer `1`, expected a string';
    },
    files_list: () => {
      throw 'state not managed for field `desk` on command `files_list`. You must call `.manage()` before using this command';
    },
    files_touch: () => {
      throw 'command files_touch not found';
    },
  });
  const s = new TauriFileStore();
  await expect(s.get(ID)).rejects.toThrow(/^Неверный запрос к хранилищу файлов$/);
  await expect(s.list('drafts', '', 0, 10)).rejects.toThrow(/^Хранилище файлов не открыто$/);
  await expect(s.touch(ID)).rejects.toThrow(/^Неверный запрос к хранилищу файлов$/);
});

test('заголовок x-ferrum — JSON только из ASCII, имя читается обратно', () => {
  const args = { name: 'Экран «Главный» 😀.svg', id: '0a1b' };
  const s = asciiJson(args);
  expect([...s].every((c) => c.charCodeAt(0) < 0x7f)).toBe(true);
  expect(JSON.parse(s)).toEqual(args);
});

test('экспорт на десктопе — export_save: байты сырым телом, имя в заголовке', async () => {
  const calls = fakeIpc({ export_save: () => true });
  const bytes = new Uint8Array([1, 2, 3, 4, 5]).subarray(1, 4);
  expect(await saveExport('Экран.pdf', bytes)).toBe(true);
  expect(calls[0].cmd).toBe('export_save');
  expect(Array.from(calls[0].args as Uint8Array)).toEqual([2, 3, 4]);
  expect(JSON.parse(calls[0].headers!['x-ferrum'])).toEqual({ name: 'Экран.pdf' });
});
