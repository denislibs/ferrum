// «Поделиться» (спека, раздел 6.3): приглашение по email с ролью, список людей, смена и снятие роли,
// ссылка с ролью и выключателем. Токен ссылки в базе — хэшем; его можно показать снова, потому что он
// выводится из секрета сервера и случайной `nonce`: утечка базы токенов не раскрывает.
// **(поведение)** Звать людей и видеть их адреса может только прямая роль или роль через команду: иначе
// пришедший по ссылке пригласил бы себя и пережил её отмену, а опубликованная ссылка раздавала бы адреса.

import {
  parseInvite,
  parseLinkUpdate,
  parseRedeem,
  parseRoleChange,
  MAX_FILES_WINDOW,
  type ShareDto,
  type ShareLinkDto,
  type ShareRole,
} from '@ferrum/protocol';
import { createHash, createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { apiError, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { inTransaction } from './db';
import { loadFile } from './file-routes';
import { accessRequestMail, fileInviteMail, sendInvite } from './invites';
import { valkeyStorage } from './secondary-storage';

const sha256 = (token: string) => createHash('sha256').update(token).digest('hex');
const DAY = 24 * 60 * 60;

export function shareRoutes(deps: ApiDeps): Route[] {
  const { pool } = deps;
  const tokenFor = (file: string, nonce: string) =>
    createHmac('sha256', deps.secret).update(`ferrum-share-link:${file}:${nonce}`).digest('base64url');

  const linkDto = (file: string, r: { role: ShareRole; enabled: boolean; nonce: string; token_hash: string }): ShareLinkDto => {
    const token = tokenFor(file, r.nonce);
    return { enabled: r.enabled, role: r.role, token: sha256(token) === r.token_hash ? token : null };
  };

  async function shareOf(ctx: Ctx): Promise<ShareDto> {
    const file = ctx.params.id;
    // Имя — только у тех, кто уже открывал файл: иначе приглашение любого адреса выдавало бы,
    // есть ли у него учётная запись и как его зовут.
    const people = await pool.query<ShareDto['people'][number]>(
      `SELECT a.id, u.email, CASE WHEN a.user_id = $2 OR EXISTS (SELECT 1 FROM file_recent r WHERE r.file_id = a.file_id AND r.user_id = a.user_id)
         THEN u.name END AS name, a.role
       FROM file_access a JOIN "user" u ON u.id = a.user_id WHERE a.file_id = $1
       UNION ALL SELECT i.id, i.email, NULL, i.role FROM file_invite i WHERE i.file_id = $1
       ORDER BY role DESC, email LIMIT $3`,
      [file, ctx.user.id, MAX_FILES_WINDOW],
    );
    const owner = ctx.role === 'owner';
    const link = owner
      ? await pool.query<{ role: ShareRole; enabled: boolean; nonce: string; token_hash: string }>(
          'SELECT role, enabled, nonce, token_hash FROM share_link WHERE file_id = $1',
          [file],
        )
      : null;
    return { people: people.rows, link: link?.rows[0] ? linkDto(file, link.rows[0]) : null, canManage: owner };
  }

  const newLink = (file: string) => {
    const nonce = randomBytes(16).toString('hex');
    return { nonce, tokenHash: sha256(tokenFor(file, nonce)) };
  };

  return [
    {
      method: 'GET',
      path: '/api/files/:id/share',
      access: { kind: 'file', min: 'editor', member: true },
      limit: 'read',
      handle: async (ctx) => ok(await shareOf(ctx)),
    },
    {
      method: 'POST',
      path: '/api/files/:id/share/invite',
      access: { kind: 'file', min: 'editor', member: true },
      limit: 'mail',
      handle: async (ctx) => {
        const body = parseInvite(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        const { rows } = await pool.query<{ id: string }>('SELECT id FROM "user" WHERE email = $1', [body.email]);
        if (rows[0]) {
          const user = rows[0].id;
          await inTransaction(pool, async (c) => {
            // Редактор может только добавить доступ, поменять чужой — владелец. Владельца это не касается.
            await c.query(
              `INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, $3)
               ON CONFLICT (file_id, user_id) DO UPDATE SET role = EXCLUDED.role
               WHERE file_access.role <> 'owner' AND ($4 OR file_access.role = 'viewer')`,
              [file, user, body.role, ctx.role === 'owner'],
            );
            // Приглашение могло остаться от гонки с первым входом: иначе в списке будет две записи.
            await c.query('DELETE FROM file_invite WHERE file_id = $1 AND email = $2', [file, body.email]);
          });
        } else {
          await pool.query(
            `INSERT INTO file_invite (file_id, email, role, invited_by) VALUES ($1, $2, $3, $4)
             ON CONFLICT (file_id, email) DO UPDATE SET role = EXCLUDED.role WHERE $5 OR file_invite.role = 'viewer'`,
            [file, body.email, body.role, ctx.user.id, ctx.role === 'owner'],
          );
        }
        const name = await pool.query<{ name: string }>('SELECT name FROM file WHERE id = $1', [file]);
        const from = ctx.user.name || ctx.user.email;
        sendInvite(deps, body.email, fileInviteMail(body.email, from, name.rows[0].name, body.role, `${deps.publicUrl}/#/file/${file}`));
        return ok(await shareOf(ctx));
      },
    },
    {
      method: 'PATCH',
      path: '/api/files/:id/share/people/:entryId',
      access: { kind: 'file', min: 'owner', member: true },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseRoleChange(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { id: file, entryId: entry } = ctx.params;
        const access = await pool.query<{ role: string }>('SELECT role FROM file_access WHERE id = $1 AND file_id = $2', [entry, file]);
        if (access.rows[0]?.role === 'owner') return apiError(403, 'FORBIDDEN');
        const changed = access.rows[0]
          ? await pool.query('UPDATE file_access SET role = $3 WHERE id = $1 AND file_id = $2', [entry, file, body.role])
          : await pool.query('UPDATE file_invite SET role = $3 WHERE id = $1 AND file_id = $2', [entry, file, body.role]);
        return changed.rowCount === 0 ? apiError(404, 'NOT_FOUND') : ok(await shareOf(ctx));
      },
    },
    {
      method: 'DELETE',
      path: '/api/files/:id/share/people/:entryId',
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      handle: async (ctx) => {
        const { id: file, entryId: entry } = ctx.params;
        const access = await pool.query<{ role: string; user_id: string }>(
          'SELECT role, user_id FROM file_access WHERE id = $1 AND file_id = $2',
          [entry, file],
        );
        const row = access.rows[0];
        const manage = ctx.role === 'owner';
        if (row) {
          // Сам себе может снять доступ каждый, кроме владельца; другому — только владелец.
          if (row.role === 'owner' || (!manage && row.user_id !== ctx.user.id)) return apiError(403, 'FORBIDDEN');
          await inTransaction(pool, async (c) => {
            await c.query('DELETE FROM file_access WHERE id = $1', [entry]);
            await c.query('DELETE FROM file_link_user WHERE file_id = $1 AND user_id = $2', [file, row.user_id]);
          });
        } else {
          const invite = await pool.query('SELECT 1 FROM file_invite WHERE id = $1 AND file_id = $2', [entry, file]);
          if (invite.rowCount === 0) return apiError(404, 'NOT_FOUND');
          if (!manage) return apiError(403, 'FORBIDDEN');
          await pool.query('DELETE FROM file_invite WHERE id = $1', [entry]);
        }
        const left = await pool.query<{ rank: number }>('SELECT file_role_rank($1, $2) AS rank', [file, ctx.user.id]);
        return left.rows[0].rank > 0 && ctx.role !== 'viewer' ? ok(await shareOf(ctx)) : ok({ ok: true });
      },
    },
    {
      method: 'PUT',
      path: '/api/files/:id/share/link',
      access: { kind: 'file', min: 'owner', member: true },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseLinkUpdate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        const { nonce, tokenHash } = newLink(file);
        await pool.query(
          `INSERT INTO share_link (file_id, token_hash, nonce, role, enabled) VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (file_id) DO UPDATE SET role = EXCLUDED.role, enabled = EXCLUDED.enabled`,
          [file, tokenHash, nonce, body.role, body.enabled],
        );
        return ok(await shareOf(ctx));
      },
    },
    {
      method: 'POST',
      path: '/api/files/:id/share/link/reset',
      access: { kind: 'file', min: 'owner', member: true },
      limit: 'write',
      handle: async (ctx) => {
        const file = ctx.params.id;
        const { nonce, tokenHash } = newLink(file);
        // Новая ссылка отменяет старую целиком: и у тех, кто уже пришёл по ней. Одной транзакцией:
        // сбой не оставит пришедших, а `redeem` ждёт её на строке ссылки.
        await inTransaction(pool, async (c) => {
          await c.query('UPDATE share_link SET token_hash = $2, nonce = $3 WHERE file_id = $1', [file, tokenHash, nonce]);
          await c.query('DELETE FROM file_link_user WHERE file_id = $1', [file]);
        });
        return ok(await shareOf(ctx));
      },
    },
    {
      // Прав на файл ещё нет: они появляются здесь. Неверный токен, выключенная ссылка и чужой файл
      // неотличимы от несуществующего.
      method: 'POST',
      path: '/api/files/:id/redeem',
      access: { kind: 'user' },
      limit: 'redeem',
      handle: async (ctx) => {
        const body = parseRedeem(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        const { rows } = await pool.query<{ token_hash: string; enabled: boolean }>(
          'SELECT token_hash, enabled FROM share_link WHERE file_id = $1',
          [file],
        );
        const want = Buffer.from(rows[0]?.token_hash ?? '0'.repeat(64));
        const hash = sha256(body.token);
        if (!rows[0]?.enabled || !timingSafeEqual(want, Buffer.from(hash))) return apiError(404, 'NOT_FOUND');
        // Условия повторяются в самой вставке под блокировкой строки ссылки: одновременная новая ссылка
        // либо уже отменила токен, либо удалит и эту запись. Файл в корзине не даёт доступа и после возврата.
        await pool.query(
          `INSERT INTO file_link_user (file_id, user_id)
           SELECT l.file_id, $2 FROM share_link l JOIN file f ON f.id = l.file_id
           WHERE l.file_id = $1 AND l.token_hash = $3 AND l.enabled AND f.trashed_at IS NULL
           FOR SHARE OF l
           ON CONFLICT DO NOTHING`,
          [file, ctx.user.id, hash],
        );
        const meta = await loadFile(pool, file, ctx.user.id);
        return meta ? ok(meta) : apiError(404, 'NOT_FOUND');
      },
    },
    {
      // Запрос доступа со страницы «Нет доступа». Ответ один и тот же, есть файл или нет.
      method: 'POST',
      path: '/api/files/:id/request-access',
      access: { kind: 'user' },
      limit: 'mail',
      handle: async (ctx) => {
        const file = ctx.params.id;
        const { rows } = await pool.query<{ name: string; email: string; rank: number; trashed: boolean }>(
          `SELECT f.name, u.email, file_role_rank(f.id, $2) AS rank, f.trashed_at IS NOT NULL AS trashed
           FROM file f JOIN "user" u ON u.id = f.owner_id WHERE f.id = $1`,
          [file, ctx.user.id],
        );
        const r = rows[0];
        if (r && r.rank === 0 && !r.trashed) {
          const key = `request-access:${file}:${ctx.user.id}`;
          const once = await valkeyStorage(deps.valkey).increment!(key, DAY);
          if (once === 1) {
            const from = ctx.user.name || ctx.user.email;
            // Письмо съел предел адреса владельца: человеку ответили «отправлено», повторить он сможет.
            sendInvite(deps, r.email, accessRequestMail(r.email, from, r.name, `${deps.publicUrl}/#/file/${file}`), () =>
              deps.valkey.del(key),
            );
          }
        }
        return ok({ ok: true });
      },
    },
  ];
}
