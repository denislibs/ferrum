// Картинки файла (спека, раздел 6.4): адресация по blake3, как в файле. Сервер проверяет хэш и размер,
// байты не держит в памяти. Доступ только через файл: `file_image` появляется только у того, кто прислал
// все байты, поэтому знание хэша чужой картинки ничего не даёт (доказательство владения вместо
// «такая уже есть»). Байты одной картинки на диске одни, сколько бы файлов на неё ни ссылалось.

import { MAX_IMAGE_BYTES } from '@ferrum/protocol';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { randomUUID } from 'node:crypto';
import { apiError, busy, ok, type ApiDeps, type Route } from './api';
import { inTransaction } from './db';
import {
  beginUpload,
  commitRefusal,
  dropUpload,
  settleUpload,
  trashBlob,
  uploadFailure,
  UPLOAD_IDLE_MS,
  UPLOAD_PACE,
  type UploadGate,
} from './file-routes';

export function imageRoutes(deps: ApiDeps, gate: UploadGate): Route[] {
  const { pool, blobs } = deps;
  const idle = deps.uploadIdleMs ?? UPLOAD_IDLE_MS;
  const pace = deps.uploadPace ?? UPLOAD_PACE;
  const linked = (file: string, hash: string) =>
    pool.query<{ blob_key: string; size: string }>(
      'SELECT i.blob_key, i.size FROM file_image l JOIN image i ON i.hash = l.hash WHERE l.file_id = $1 AND l.hash = $2',
      [file, hash],
    );
  return [
    {
      // Есть ли картинка у этого файла: клиент не шлёт байты второй раз.
      method: 'HEAD',
      path: '/api/files/:id/images/:hash',
      access: { kind: 'file', min: 'viewer' },
      limit: 'images',
      handle: async (ctx) => {
        const { rows } = await linked(ctx.params.id, ctx.params.hash);
        return new Response(null, {
          status: rows[0] ? 200 : 404,
          headers: { 'Cache-Control': 'no-store', ...(rows[0] ? { 'Content-Length': rows[0].size } : {}) },
        });
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/images/:hash',
      access: { kind: 'file', min: 'viewer' },
      limit: 'images',
      handle: async (ctx) => {
        const { rows } = await linked(ctx.params.id, ctx.params.hash);
        if (!rows[0]) return apiError(404, 'NOT_FOUND');
        const blob = await blobs.open(rows[0].blob_key);
        if (!blob) throw new Error(`нет байтов картинки ${ctx.params.hash}`);
        // Тип не угадывается и не отдаётся картинкой: это байты для ядра, а не страница браузера.
        return new Response(blob, {
          headers: {
            'Content-Type': 'application/octet-stream',
            'Content-Disposition': 'attachment',
            'Cache-Control': 'no-store',
            'Content-Length': String(blob.size),
          },
        });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/images/:hash',
      access: { kind: 'file', min: 'editor' },
      limit: 'images',
      stream: true,
      handle: async (ctx) => {
        if (!ctx.req.body) return apiError(400, 'BAD_REQUEST');
        if (Number(ctx.req.headers.get('content-length') ?? 0) > MAX_IMAGE_BYTES) return apiError(413, 'TOO_LARGE');
        if (!gate.enter(ctx.user.id)) return busy();
        const key = `images/${ctx.params.hash}/${randomUUID()}`;
        try {
          await beginUpload(pool, key);
          const hasher = blake3.create();
          let size: number;
          try {
            ({ size } = await blobs.putStream(key, ctx.req.body, {
              maxBytes: MAX_IMAGE_BYTES,
              idleMs: idle,
              pace,
              onChunk: (chunk) => void hasher.update(chunk),
            }));
          } catch (e) {
            await dropUpload(deps, key).catch(() => {});
            const failure = uploadFailure(e);
            if (failure) return failure;
            throw e;
          }
          if (bytesToHex(hasher.digest()) !== ctx.params.hash) {
            await dropUpload(deps, key).catch(() => {});
            return apiError(400, 'HASH_MISMATCH');
          }
          // Пока байты шли, файл могли удалить, а роль снять: решает только эта транзакция.
          const outcome = await inTransaction(pool, async (c) => {
            const file = await c.query<{ rank: number }>('SELECT file_role_rank(id, $2) AS rank FROM file WHERE id = $1 FOR UPDATE', [
              ctx.params.id,
              ctx.user.id,
            ]);
            const settled = await settleUpload(c, key, commitRefusal(file.rows[0]));
            if (settled) return settled;
            // Такая уже есть: новые байты лишние, а запись оживает, даже если её вот-вот удалила бы очистка.
            const { rows } = await c.query<{ blob_key: string }>(
              `INSERT INTO image (hash, size, blob_key) VALUES ($1, $2, $3)
               ON CONFLICT (hash) DO UPDATE SET orphaned_at = NULL RETURNING blob_key`,
              [ctx.params.hash, size, key],
            );
            if (rows[0].blob_key !== key) await trashBlob(c, key);
            // Срок идёт с загрузки, его снимает только запись содержимого со ссылкой (спека, 6.4: сначала
            // картинка, потом правка). Повторная загрузка срок не трогает: иначе его можно продлевать вечно.
            await c.query('INSERT INTO file_image (file_id, hash) VALUES ($1, $2) ON CONFLICT (file_id, hash) DO NOTHING', [
              ctx.params.id,
              ctx.params.hash,
            ]);
            return null;
          });
          if (outcome) {
            await blobs.delete(key).catch(() => {});
            return outcome;
          }
          return ok({ ok: true });
        } finally {
          gate.leave(ctx.user.id);
        }
      },
    },
  ];
}
