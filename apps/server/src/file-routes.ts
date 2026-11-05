// Файлы: списки, создание, имя, корзина, содержимое с проверкой поколения, превью (спека, раздел 6).
// Содержимое идёт потоком на диск и обратно: сервер не держит файл в памяти, а замена — одна
// транзакция с проверкой поколения и роли на момент фиксации.

import {
  ContainerError,
  ContainerWalker,
  TooManyImages,
  MAX_CONTENT_BYTES,
  MAX_THUMB_BYTES,
  RANK_ROLE,
  ROLE_RANK,
  VERSION_HEADER,
  parseCreateFile,
  parseListQuery,
  parseMove,
  parseRename,
  parseWriteHeaders,
  type FileAccessDto,
  type FileListDto,
  type FileMetaDto,
  type FileRole,
  type ListQuery,
} from '@ferrum/protocol';
import { randomUUID } from 'node:crypto';
import type { Pool, PoolClient } from 'pg';
import { apiError, busy, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { BlobStalled, BlobTooLarge, type UploadPace } from './blob-store';
import { inTransaction } from './db';

/** Клиент молчит дольше этого посреди загрузки — обрыв. */
export const UPLOAD_IDLE_MS = 60_000;
/** Загрузка медленнее этого держит слот зря: 512 МБ укладываются в 4,6 часа, вдвое дольше срока клиента
 * (`transferTimeout`, от 64 КиБ/с), поэтому законную загрузку на медленном канале сервер не обрывает. */
export const UPLOAD_PACE: UploadPace = { graceMs: 60_000, bytesPerSecond: 32 * 1024 };
/** Байты заменённого файла удаляются не сразу: читатель, начавший раньше, успевает дочитать. */
export const BLOB_GRACE_SECONDS = 120;
/** Одновременных загрузок: на человека и на сервер. Памяти загрузка не занимает, занимает диск и поток. */
export const MAX_UPLOADS_PER_USER = 8;
export const MAX_UPLOADS = 32;
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

export const trashBlob = (c: PoolClient, key: string | null) =>
  key
    ? c.query(`INSERT INTO blob_trash (key, delete_after) VALUES ($1, now() + make_interval(secs => $2)) ON CONFLICT DO NOTHING`, [
        key,
        BLOB_GRACE_SECONDS,
      ])
    : Promise.resolve();

/** Намерение: до первого байта. Без него сбой транзакции или процесса оставил бы байты навсегда. */
export const beginUpload = (pool: Pool, key: string) => pool.query('INSERT INTO blob_pending (key) VALUES ($1)', [key]);

/** Загрузка оборвалась до транзакции: ни байтов, ни намерения. Не удалось — останется очистке по сроку. */
export async function dropUpload(deps: ApiDeps, key: string): Promise<void> {
  await deps.blobs.delete(key);
  await deps.pool.query('DELETE FROM blob_pending WHERE key = $1', [key]);
}

/** Решение транзакции фиксации. Отказ — байты уходят в очистку той же транзакцией, вызывающий
 * возвращает ответ. Намерения уже нет — очистка сочла загрузку брошенной, байты тоже не жильцы. */
export async function settleUpload(c: PoolClient, key: string, refusal: Response | null): Promise<Response | null> {
  const { rowCount } = await c.query('DELETE FROM blob_pending WHERE key = $1', [key]);
  const out = refusal ?? (rowCount === 0 ? apiError(503, 'UNAVAILABLE') : null);
  if (out) await trashBlob(c, key);
  return out;
}

/** Файл и роль на момент фиксации: пока шли байты, его могли удалить, а роль снять. */
export function commitRefusal(row: { rank: number } | undefined): Response | null {
  if (!row || row.rank === 0) return apiError(404, 'NOT_FOUND');
  return row.rank < ROLE_RANK.editor ? apiError(403, 'FORBIDDEN') : null;
}

interface FileRow {
  id: string;
  name: string;
  created_at: Date;
  edited_at: Date;
  trashed_at: Date | null;
  size: string;
  thumb_version: number;
  version: string;
  project_id: string | null;
  opened_at: Date | null;
  rank: number;
}

/** Колонки записи о файле; `$1` запроса — id человека. */
const FILE_COLUMNS = `f.id, f.name, f.created_at, f.edited_at, f.trashed_at, f.size, f.thumb_version, f.version, f.project_id,
  (SELECT r.opened_at FROM file_recent r WHERE r.user_id = $1 AND r.file_id = f.id) AS opened_at,
  file_role_rank(f.id, $1) AS rank`;

export function fileDto(r: FileRow): FileMetaDto {
  return {
    id: r.id,
    name: r.name,
    createdAt: r.created_at.getTime(),
    editedAt: r.edited_at.getTime(),
    openedAt: r.opened_at?.getTime() ?? 0,
    size: Number(r.size),
    thumbVersion: r.thumb_version,
    version: Number(r.version),
    role: RANK_ROLE[r.rank] as FileRole,
    ...(r.trashed_at ? { trashedAt: r.trashed_at.getTime() } : {}),
    ...(r.project_id ? { projectId: r.project_id } : {}),
  };
}

export async function loadFile(db: Pool | PoolClient, id: string, user: string): Promise<FileMetaDto | null> {
  const { rows } = await db.query<FileRow>(`SELECT ${FILE_COLUMNS} FROM file f WHERE f.id = $2`, [user, id]);
  // Нет роли — нет и записи: файл только что убрали в корзину или сняли доступ.
  return rows[0] && rows[0].rank > 0 ? fileDto(rows[0]) : null;
}

/** Условие раздела: только файлы, доступные этому человеку. `$1` — его id, `$2` — проект. */
function sectionWhere(q: ListQuery): { from: string; where: string; order: string; project: string | null } {
  const project = q.section.startsWith('project:') ? q.section.slice(8) : null;
  switch (q.section) {
    case 'recent':
      return {
        from: 'file f JOIN file_recent r ON r.file_id = f.id AND r.user_id = $1',
        where: 'f.trashed_at IS NULL AND file_role_rank(f.id, $1) > 0',
        order: 'r.opened_at DESC',
        project,
      };
    case 'drafts':
      return {
        from: 'file f',
        where: 'f.owner_id = $1 AND f.project_id IS NULL AND f.trashed_at IS NULL',
        order: 'f.edited_at DESC',
        project,
      };
    case 'shared':
      return {
        from: 'file f',
        where: `f.trashed_at IS NULL AND f.owner_id <> $1 AND (
          EXISTS (SELECT 1 FROM file_access a WHERE a.file_id = f.id AND a.user_id = $1)
          OR EXISTS (SELECT 1 FROM file_link_user u JOIN share_link l ON l.file_id = u.file_id AND l.enabled
                     WHERE u.file_id = f.id AND u.user_id = $1))`,
        order: 'f.edited_at DESC',
        project,
      };
    case 'trash':
      return {
        from: 'file f',
        where: 'f.trashed_at IS NOT NULL AND (f.owner_id = $1 OR f.trashed_by = $1) AND file_role_rank(f.id, $1) = 3',
        order: 'f.trashed_at DESC',
        project,
      };
    default:
      return {
        from: 'file f',
        where: `f.trashed_at IS NULL AND f.project_id = $2 AND EXISTS (
          SELECT 1 FROM project p JOIN team_member m ON m.team_id = p.team_id AND m.user_id = $1 WHERE p.id = f.project_id)`,
        order: 'f.edited_at DESC',
        project,
      };
  }
}

const escapeLike = (s: string) => s.replace(/[\\%_]/g, (c) => `\\${c}`);

/** Счётчики одновременных загрузок: живут в процессе, сервер один. */
export function uploadGate() {
  const perUser = new Map<string, number>();
  let total = 0;
  return {
    enter(user: string): boolean {
      const mine = perUser.get(user) ?? 0;
      if (mine >= MAX_UPLOADS_PER_USER || total >= MAX_UPLOADS) return false;
      perUser.set(user, mine + 1);
      total++;
      return true;
    },
    leave(user: string): void {
      const mine = (perUser.get(user) ?? 1) - 1;
      if (mine > 0) perUser.set(user, mine);
      else perUser.delete(user);
      total--;
    },
  };
}
export type UploadGate = ReturnType<typeof uploadGate>;

/** Ответ на неудачную загрузку потоком. Неизвестное — сбой маршрута, его показывает конвейер. */
export function uploadFailure(e: unknown): Response | null {
  if (e instanceof BlobTooLarge) return apiError(413, 'TOO_LARGE');
  if (e instanceof TooManyImages) return apiError(400, 'TOO_MANY_IMAGES');
  if (e instanceof ContainerError) return apiError(400, 'BAD_CONTAINER');
  if (e instanceof BlobStalled) return apiError(408, 'UNAVAILABLE');
  return null;
}

/** Своя запись, ответ на которую потерялся, — не чужая: поколение ровно на одну больше и она последняя. */
function writeAllowed(row: { version: string; last_write_id: string | null }, base: number, previous: string | null): boolean {
  const version = Number(row.version);
  return version === base || (previous !== null && row.last_write_id === previous && version === base + 1);
}

/** Запись не оставляет ссылку без байтов: очистка могла снять картинку, пока клиент считал её известной.
 * Строки под блокировкой: очистка, начатая раньше, уже прошла, а начатая позже увидит снятый срок. */
async function missingImages(c: PoolClient, file: string, refs: string[], lost: number): Promise<Response | null> {
  if (refs.length === 0) return null;
  const { rowCount } = await c.query('SELECT 1 FROM file_image WHERE file_id = $1 AND hash = ANY($2::text[]) FOR UPDATE', [file, refs]);
  return refs.length - (rowCount ?? 0) > lost ? apiError(409, 'MISSING_IMAGES') : null;
}

export function fileRoutes(deps: ApiDeps, gate: UploadGate): Route[] {
  const { pool, blobs } = deps;
  const idle = deps.uploadIdleMs ?? UPLOAD_IDLE_MS;
  const pace = deps.uploadPace ?? UPLOAD_PACE;
  const maxContent = deps.maxContentBytes ?? MAX_CONTENT_BYTES;
  const fresh = async (ctx: Ctx) => {
    const meta = await loadFile(pool, ctx.params.id, ctx.user.id);
    return meta ? ok(meta) : apiError(404, 'NOT_FOUND');
  };
  return [
    {
      method: 'GET',
      path: '/api/files',
      access: { kind: 'user' },
      limit: 'read',
      handle: async (ctx) => {
        const q = parseListQuery(ctx.url.searchParams);
        if (!q) return apiError(400, 'BAD_REQUEST');
        const s = sectionWhere(q);
        const params: unknown[] = [ctx.user.id];
        if (s.project) params.push(s.project);
        let where = s.where;
        if (q.query) {
          params.push(`%${escapeLike(q.query)}%`);
          where += ` AND f.name ILIKE $${params.length}`;
        }
        const total = await pool.query<{ n: string }>(`SELECT count(*) AS n FROM ${s.from} WHERE ${where}`, params);
        const rows = await pool.query<FileRow>(
          `SELECT ${FILE_COLUMNS} FROM ${s.from} WHERE ${where} ORDER BY ${s.order}, f.name, f.id
           LIMIT $${params.length + 1} OFFSET $${params.length + 2}`,
          [...params, q.count, q.start],
        );
        return ok({ total: Number(total.rows[0].n), rows: rows.rows.filter((r) => r.rank > 0).map(fileDto) } satisfies FileListDto);
      },
    },
    {
      method: 'POST',
      path: '/api/files',
      access: { kind: 'user' },
      limit: 'create',
      handle: async (ctx) => {
        const body = parseCreateFile(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        if (body.projectId) {
          // Чужой проект неотличим от несуществующего.
          const member = await pool.query(
            'SELECT 1 FROM project p JOIN team_member m ON m.team_id = p.team_id WHERE p.id = $1 AND m.user_id = $2',
            [body.projectId, ctx.user.id],
          );
          if (member.rowCount === 0) return apiError(404, 'NOT_FOUND');
        }
        const meta = await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ id: string }>('INSERT INTO file (owner_id, project_id, name) VALUES ($1, $2, $3) RETURNING id', [
            ctx.user.id,
            body.projectId,
            body.name,
          ]);
          const id = rows[0].id;
          await c.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'owner')`, [id, ctx.user.id]);
          await c.query('INSERT INTO file_recent (user_id, file_id, opened_at) VALUES ($1, $2, now())', [ctx.user.id, id]);
          return loadFile(c, id, ctx.user.id);
        });
        return ok(meta, 201);
      },
    },
    {
      method: 'POST',
      path: '/api/files/empty-trash',
      access: { kind: 'user' },
      limit: 'write',
      handle: async (ctx) => {
        await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ id: string; content_key: string | null; thumb_key: string | null }>(
            `SELECT id, content_key, thumb_key FROM file
             WHERE trashed_at IS NOT NULL AND (owner_id = $1 OR trashed_by = $1) AND file_role_rank(id, $1) = 3 FOR UPDATE`,
            [ctx.user.id],
          );
          for (const r of rows) {
            await trashBlob(c, r.content_key);
            await trashBlob(c, r.thumb_key);
          }
          await c.query('DELETE FROM file WHERE id = ANY($1::uuid[])', [rows.map((r) => r.id)]);
        });
        return ok({ ok: true });
      },
    },
    { method: 'GET', path: '/api/files/:id', access: { kind: 'file', min: 'viewer' }, limit: 'read', handle: fresh },
    {
      method: 'PATCH',
      path: '/api/files/:id',
      access: { kind: 'file', min: 'editor' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseRename(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        await pool.query('UPDATE file SET name = $2, edited_at = now() WHERE id = $1', [ctx.params.id, body.name]);
        return fresh(ctx);
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/move',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseMove(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        if (body.projectId) {
          const member = await pool.query(
            'SELECT 1 FROM project p JOIN team_member m ON m.team_id = p.team_id WHERE p.id = $1 AND m.user_id = $2',
            [body.projectId, ctx.user.id],
          );
          if (member.rowCount === 0) return apiError(404, 'NOT_FOUND');
        }
        await pool.query('UPDATE file SET project_id = $2 WHERE id = $1', [ctx.params.id, body.projectId]);
        return fresh(ctx);
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/touch',
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      handle: async (ctx) => {
        await pool.query(
          `INSERT INTO file_recent (user_id, file_id, opened_at) VALUES ($1, $2, now())
           ON CONFLICT (user_id, file_id) DO UPDATE SET opened_at = now()`,
          [ctx.user.id, ctx.params.id],
        );
        return fresh(ctx);
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/trash',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        await pool.query('UPDATE file SET trashed_at = now(), trashed_by = $2 WHERE id = $1 AND trashed_at IS NULL', [
          ctx.params.id,
          ctx.user.id,
        ]);
        return ok({ ok: true });
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/restore',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        await pool.query('UPDATE file SET trashed_at = NULL, trashed_by = NULL WHERE id = $1', [ctx.params.id]);
        return ok({ ok: true });
      },
    },
    {
      method: 'DELETE',
      path: '/api/files/:id',
      access: { kind: 'file', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ content_key: string | null; thumb_key: string | null }>(
            'DELETE FROM file WHERE id = $1 RETURNING content_key, thumb_key',
            [ctx.params.id],
          );
          await trashBlob(c, rows[0]?.content_key ?? null);
          await trashBlob(c, rows[0]?.thumb_key ?? null);
        });
        return ok({ ok: true });
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/access',
      access: { kind: 'file', min: 'viewer' },
      limit: 'poll',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ version: string; member: number }>(
          'SELECT version, file_member_rank(id, $2) AS member FROM file WHERE id = $1',
          [ctx.params.id, ctx.user.id],
        );
        if (!rows[0]) return apiError(404, 'NOT_FOUND');
        return ok({
          role: ctx.role,
          version: Number(rows[0].version),
          canShare: rows[0].member >= ROLE_RANK.editor,
        } satisfies FileAccessDto);
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/content',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ version: string; content_key: string | null }>(
          'SELECT version, content_key FROM file WHERE id = $1',
          [ctx.params.id],
        );
        const row = rows[0];
        if (!row) return apiError(404, 'NOT_FOUND');
        const headers = { [VERSION_HEADER]: row.version, 'Cache-Control': 'no-store' };
        if (!row.content_key) return new Response(null, { status: 204, headers });
        const blob = await blobs.open(row.content_key);
        // Запись в базе есть, а байтов нет: не «файл не найден», а сбой хранилища.
        if (!blob) throw new Error(`нет байтов содержимого файла ${ctx.params.id}`);
        return new Response(blob, {
          headers: {
            ...headers,
            'Content-Type': 'application/octet-stream',
            'Content-Disposition': 'attachment',
            'Content-Length': String(blob.size),
            'Cross-Origin-Resource-Policy': 'same-origin',
          },
        });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/content',
      access: { kind: 'file', min: 'editor' },
      limit: 'upload',
      stream: true,
      handle: async (ctx) => {
        const write = parseWriteHeaders(ctx.req.headers);
        if (!write || !ctx.req.body) return apiError(400, 'BAD_REQUEST');
        const declared = Number(ctx.req.headers.get('content-length') ?? 0);
        if (declared > maxContent) return apiError(413, 'TOO_LARGE');
        // Заведомый отказ — раньше загрузки: не занимать диск и не гонять байты зря.
        const before = await pool.query<{ version: string; last_write_id: string | null }>(
          'SELECT version, last_write_id FROM file WHERE id = $1',
          [ctx.params.id],
        );
        if (!before.rows[0]) return apiError(404, 'NOT_FOUND');
        if (!writeAllowed(before.rows[0], write.base, write.previous)) return apiError(409, 'CONFLICT');
        if (!gate.enter(ctx.user.id)) return busy();
        const key = `files/${ctx.params.id}/content-${randomUUID()}`;
        try {
          await beginUpload(pool, key);
          const walker = new ContainerWalker();
          let size: number;
          try {
            ({ size } = await blobs.putStream(key, ctx.req.body, {
              maxBytes: maxContent,
              idleMs: idle,
              pace,
              onChunk: (chunk) => walker.feed(chunk),
            }));
            walker.finish();
          } catch (e) {
            await dropUpload(deps, key).catch(() => {});
            const failure = uploadFailure(e);
            if (failure) return failure;
            throw e;
          }
          const refs = [...walker.imageRefs];
          // Пока байты шли, файл могли записать, а роль снять: решает только эта транзакция.
          const outcome = await inTransaction(pool, async (c) => {
            const { rows } = await c.query<{ version: string; last_write_id: string | null; content_key: string | null; rank: number }>(
              'SELECT version, last_write_id, content_key, file_role_rank(id, $2) AS rank FROM file WHERE id = $1 FOR UPDATE',
              [ctx.params.id, ctx.user.id],
            );
            const row = rows[0];
            const refused =
              commitRefusal(row) ??
              (writeAllowed(row, write.base, write.previous) ? null : apiError(409, 'CONFLICT')) ??
              (await missingImages(c, ctx.params.id, refs, write.lost));
            const settled = await settleUpload(c, key, refused);
            if (settled) return settled;
            await trashBlob(c, row.content_key);
            await c.query(
              `UPDATE file SET version = version + 1, size = $2, content_key = $3, last_write_id = $4, edited_at = now() WHERE id = $1`,
              [ctx.params.id, size, key, write.write],
            );
            // Ссылки из содержимого: картинка без ссылки живёт ещё 30 дней (отмена может её вернуть).
            await c.query(
              `UPDATE file_image SET unreferenced_at = CASE WHEN hash = ANY($2::text[]) THEN NULL ELSE COALESCE(unreferenced_at, now()) END
               WHERE file_id = $1`,
              [ctx.params.id, refs],
            );
            return null;
          });
          if (outcome) {
            await blobs.delete(key).catch(() => {});
            return outcome;
          }
          return await fresh(ctx);
        } finally {
          gate.leave(ctx.user.id);
        }
      },
    },
    {
      method: 'GET',
      path: '/api/files/:id/thumb',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      handle: async (ctx) => {
        const { rows } = await pool.query<{ thumb_key: string | null }>('SELECT thumb_key FROM file WHERE id = $1', [ctx.params.id]);
        const blob = rows[0]?.thumb_key ? await blobs.open(rows[0].thumb_key) : null;
        if (!blob) return new Response(null, { status: 204, headers: { 'Cache-Control': 'no-store' } });
        return new Response(blob, {
          headers: {
            'Content-Type': 'image/png',
            'Cache-Control': 'no-store',
            'Content-Length': String(blob.size),
            'Cross-Origin-Resource-Policy': 'same-origin',
          },
        });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/thumb',
      access: { kind: 'file', min: 'editor' },
      limit: 'upload',
      stream: true,
      handle: async (ctx) => {
        if (!ctx.req.body) return apiError(400, 'BAD_REQUEST');
        if (!gate.enter(ctx.user.id)) return busy();
        const key = `files/${ctx.params.id}/thumb-${randomUUID()}`;
        try {
          await beginUpload(pool, key);
          let seen = 0;
          try {
            await blobs.putStream(key, ctx.req.body, {
              maxBytes: MAX_THUMB_BYTES,
              idleMs: idle,
              pace,
              onChunk: (chunk) => {
                for (let i = 0; i < chunk.length && seen < PNG_SIGNATURE.length; i++, seen++) {
                  if (chunk[i] !== PNG_SIGNATURE[seen]) throw new ContainerError('не PNG');
                }
              },
            });
            if (seen < PNG_SIGNATURE.length) throw new ContainerError('не PNG');
          } catch (e) {
            await dropUpload(deps, key).catch(() => {});
            const failure = uploadFailure(e);
            if (failure) return failure;
            throw e;
          }
          // Пока байты шли, файл могли удалить, а роль снять: решает только эта транзакция.
          const outcome = await inTransaction(pool, async (c) => {
            const { rows } = await c.query<{ thumb_key: string | null; rank: number }>(
              'SELECT thumb_key, file_role_rank(id, $2) AS rank FROM file WHERE id = $1 FOR UPDATE',
              [ctx.params.id, ctx.user.id],
            );
            const settled = await settleUpload(c, key, commitRefusal(rows[0]));
            if (settled) return settled;
            await trashBlob(c, rows[0].thumb_key);
            await c.query('UPDATE file SET thumb_key = $2, thumb_version = thumb_version + 1 WHERE id = $1', [ctx.params.id, key]);
            return null;
          });
          if (outcome) {
            await blobs.delete(key).catch(() => {});
            return outcome;
          }
          return await fresh(ctx);
        } finally {
          gate.leave(ctx.user.id);
        }
      },
    },
    {
      method: 'DELETE',
      path: '/api/files/:id/thumb',
      access: { kind: 'file', min: 'editor' },
      limit: 'write',
      handle: async (ctx) => {
        await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ thumb_key: string | null }>('SELECT thumb_key FROM file WHERE id = $1 FOR UPDATE', [
            ctx.params.id,
          ]);
          if (!rows[0]?.thumb_key) return;
          await trashBlob(c, rows[0].thumb_key);
          await c.query('UPDATE file SET thumb_key = NULL, thumb_version = thumb_version + 1 WHERE id = $1', [ctx.params.id]);
        });
        return fresh(ctx);
      },
    },
  ];
}
