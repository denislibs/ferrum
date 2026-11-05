import { afterAll, beforeEach, expect, test } from 'bun:test';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { runGc } from '../src/gc';
import { apiStand, container, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const other = await s.signIn('other@example.com');
beforeEach(s.reset);

const hashOf = (b: Uint8Array) => bytesToHex(blake3(b));
const bytes = (fill: number, size = 100): Uint8Array<ArrayBuffer> => new Uint8Array(size).fill(fill);
const put = (who: Person, file: string, hash: string, body: Uint8Array) => s.call(who, 'PUT', `/api/files/${file}/images/${hash}`, body);
const get = (who: Person, file: string, hash: string) => s.call(who, 'GET', `/api/files/${file}/images/${hash}`);
const head = (who: Person, file: string, hash: string) => s.call(who, 'HEAD', `/api/files/${file}/images/${hash}`);

test('загрузка и чтение: хэш сверяется, байты те же, HEAD знает о картинке', async () => {
  const f = await s.newFile(owner);
  const img = bytes(1);
  const hash = hashOf(img);
  expect((await head(owner, f.id, hash)).status).toBe(404);
  expect((await put(owner, f.id, hash, img)).status).toBe(200);
  expect((await head(owner, f.id, hash)).status).toBe(200);
  const res = await get(owner, f.id, hash);
  expect(res.headers.get('content-type')).toBe('application/octet-stream');
  expect(res.headers.get('content-disposition')).toBe('attachment');
  expect(new Uint8Array(await res.arrayBuffer())).toEqual(img);
});

test('байты не совпали с хэшем: 400, ни записи, ни байтов', async () => {
  const f = await s.newFile(owner);
  const res = await put(owner, f.id, hashOf(bytes(1)), bytes(2));
  expect(res.status).toBe(400);
  expect(await res.json()).toEqual({ error: 'HASH_MISMATCH' });
  expect((await s.pool.query('SELECT 1 FROM image')).rowCount).toBe(0);
  expect((await head(owner, f.id, hashOf(bytes(1)))).status).toBe(404);
});

test('сверх предела размера: 413 по `Content-Length`', async () => {
  const f = await s.newFile(owner);
  const res = await fetch(`${s.publicUrl}/api/files/${f.id}/images/${'a'.repeat(64)}`, {
    method: 'PUT',
    headers: { origin: s.publicUrl, cookie: owner.cookie },
    body: new Uint8Array(65 * 1024 * 1024),
  });
  expect(res.status).toBe(413);
  expect(await res.json()).toEqual({ error: 'TOO_LARGE' });
});

test('знание хэша ничего не даёт: чужая картинка не читается и без байтов не привязывается', async () => {
  const mine = await s.newFile(owner);
  const theirs = await s.newFile(other);
  const img = bytes(3);
  const hash = hashOf(img);
  await put(owner, mine.id, hash, img);
  // Через свой файл: ссылки нет, 404.
  expect((await get(other, theirs.id, hash)).status).toBe(404);
  expect((await head(other, theirs.id, hash)).status).toBe(404);
  // Через чужой файл: нет доступа, 404.
  expect((await get(other, mine.id, hash)).status).toBe(404);
  // Содержимое со ссылкой на чужой хэш не привязывает картинку.
  expect((await s.writeContent(other, theirs.id, 0, container([hash]))).status).toBe(200);
  expect((await get(other, theirs.id, hash)).status).toBe(404);
  // Прислать все байты — значит иметь картинку: тогда ссылка есть, а запись на диске одна.
  expect((await put(other, theirs.id, hash, img)).status).toBe(200);
  expect((await get(other, theirs.id, hash)).status).toBe(200);
  const { rows } = await s.pool.query('SELECT blob_key FROM image');
  expect(rows).toHaveLength(1);
  const trash = await s.pool.query('SELECT key FROM blob_trash');
  expect(trash.rows).toHaveLength(1);
  expect(trash.rows[0].key).not.toBe(rows[0].blob_key);
});

const age = (table: string, column: string, days: number) =>
  s.pool.query(`UPDATE ${table} SET ${column} = now() - make_interval(days => ${days})`);

test('очистка: картинка без ссылок живёт 30 дней, отмена её возвращает', async () => {
  const f = await s.newFile(owner);
  const img = bytes(4);
  const hash = hashOf(img);
  await put(owner, f.id, hash, img);
  await s.writeContent(owner, f.id, 0, container([hash]));
  expect((await s.pool.query('SELECT unreferenced_at FROM file_image')).rows[0].unreferenced_at).toBeNull();
  // Ссылка пропала из содержимого: отметка, но чтение работает.
  await s.writeContent(owner, f.id, 1, container());
  expect((await s.pool.query('SELECT unreferenced_at FROM file_image')).rows[0].unreferenced_at).not.toBeNull();
  expect((await get(owner, f.id, hash)).status).toBe(200);
  // Ссылка вернулась (отмена): отметка снята.
  await s.writeContent(owner, f.id, 2, container([hash]));
  expect((await s.pool.query('SELECT unreferenced_at FROM file_image')).rows[0].unreferenced_at).toBeNull();
  await s.writeContent(owner, f.id, 3, container());
  await age('file_image', 'unreferenced_at', 29);
  await runGc(s.pool, s.blobs);
  expect((await s.pool.query('SELECT 1 FROM file_image')).rowCount).toBe(1);
  await age('file_image', 'unreferenced_at', 31);
  await runGc(s.pool, s.blobs);
  expect((await s.pool.query('SELECT 1 FROM file_image')).rowCount).toBe(0);
  expect((await head(owner, f.id, hash)).status).toBe(404);
  // Запись самой картинки ещё держится свои 30 дней.
  const { rows } = await s.pool.query('SELECT blob_key, orphaned_at FROM image');
  expect(rows[0].orphaned_at).not.toBeNull();
  expect(await s.blobs.get(rows[0].blob_key)).not.toBeNull();
  await age('image', 'orphaned_at', 31);
  await runGc(s.pool, s.blobs);
  expect((await s.pool.query('SELECT 1 FROM image')).rowCount).toBe(0);
  expect(await s.blobs.get(rows[0].blob_key)).toBeNull();
});

test('очистка не трогает картинку другого файла; удаление файла снимает только его ссылку', async () => {
  const a = await s.newFile(owner);
  const b = await s.newFile(owner);
  const img = bytes(5);
  const hash = hashOf(img);
  await put(owner, a.id, hash, img);
  await put(owner, b.id, hash, img);
  await s.call(owner, 'DELETE', `/api/files/${a.id}`);
  await s.pool.query(`UPDATE image SET orphaned_at = now() - interval '40 days'`);
  await runGc(s.pool, s.blobs);
  // Картинка нужна второму файлу: отметка снята, байты на месте.
  expect((await s.pool.query('SELECT orphaned_at FROM image')).rows[0].orphaned_at).toBeNull();
  expect((await get(owner, b.id, hash)).status).toBe(200);
});

test('повторная загрузка той же картинки оживляет запись перед удалением', async () => {
  const f = await s.newFile(owner);
  const img = bytes(6);
  const hash = hashOf(img);
  await put(owner, f.id, hash, img);
  await s.call(owner, 'DELETE', `/api/files/${f.id}`);
  await runGc(s.pool, s.blobs);
  await age('image', 'orphaned_at', 29);
  const g = await s.newFile(owner);
  expect((await put(owner, g.id, hash, img)).status).toBe(200);
  await age('image', 'orphaned_at', 40);
  await runGc(s.pool, s.blobs);
  expect((await get(owner, g.id, hash)).status).toBe(200);
});
