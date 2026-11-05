// Команды и проекты (спека, раздел 6.1, 6.2). Команду создаёт любой человек и становится её владельцем.
// Участники — по email: человек без учётной записи попадает в команду при первом входе
// (`claimInvites`). Участник — редактор файлов проектов команды, владелец команды — владелец.

import {
  MAX_FILES_WINDOW,
  parseTeamCreate,
  parseTeamInvite,
  type ProjectDto,
  type TeamDto,
  type TeamListDto,
  type TeamMembersDto,
} from '@ferrum/protocol';
import type { PoolClient } from 'pg';
import { apiError, ok, readJson, type ApiDeps, type Ctx, type Route } from './api';
import { inTransaction } from './file-routes';
import { sendInvite, teamInviteMail } from './invites';

/** Кто в команде: и участники, и приглашённые. Только адреса: список не выдаёт, есть ли учётная запись. */
async function members(deps: ApiDeps, teamId: string): Promise<TeamMembersDto> {
  const { rows } = await deps.pool.query<TeamMembersDto['members'][number]>(
    `SELECT m.id, u.email, m.role FROM team_member m JOIN "user" u ON u.id = m.user_id WHERE m.team_id = $1
     UNION ALL SELECT i.id, i.email, 'member' FROM team_invite i WHERE i.team_id = $1
     ORDER BY role DESC, email LIMIT $2`,
    [teamId, MAX_FILES_WINDOW],
  );
  return { members: rows };
}

/** Файлы ушедшего из проектов команды остаются у команды: владелец — владелец команды. */
async function handOverFiles(c: PoolClient, teamId: string, from: string, to: string): Promise<void> {
  const { rows } = await c.query<{ id: string }>(
    `UPDATE file f SET owner_id = $3 FROM project p
     WHERE f.project_id = p.id AND p.team_id = $1 AND f.owner_id = $2 RETURNING f.id`,
    [teamId, from, to],
  );
  const ids = rows.map((r) => r.id);
  await c.query('DELETE FROM file_access WHERE file_id = ANY($1::uuid[]) AND user_id = $2', [ids, from]);
  await c.query(
    `INSERT INTO file_access (file_id, user_id, role) SELECT unnest($1::uuid[]), $2, 'owner'
     ON CONFLICT (file_id, user_id) DO UPDATE SET role = 'owner'`,
    [ids, to],
  );
}

export function teamRoutes(deps: ApiDeps): Route[] {
  const { pool } = deps;
  const projectDto = (r: { id: string; team_id: string; name: string }): ProjectDto => ({ id: r.id, teamId: r.team_id, name: r.name });
  return [
    {
      method: 'GET',
      path: '/api/teams',
      access: { kind: 'user' },
      limit: 'read',
      handle: async (ctx) => {
        const teams = await pool.query<{ id: string; name: string; role: 'owner' | 'member' }>(
          `SELECT t.id, t.name, m.role FROM team_member m JOIN team t ON t.id = m.team_id
           WHERE m.user_id = $1 ORDER BY t.name, t.id LIMIT $2`,
          [ctx.user.id, MAX_FILES_WINDOW],
        );
        const projects = await pool.query<{ id: string; team_id: string; name: string }>(
          'SELECT id, team_id, name FROM project WHERE team_id = ANY($1::uuid[]) ORDER BY name, id',
          [teams.rows.map((t) => t.id)],
        );
        const out: TeamDto[] = teams.rows.map((t) => ({ ...t, projects: projects.rows.filter((p) => p.team_id === t.id).map(projectDto) }));
        return ok({ teams: out } satisfies TeamListDto);
      },
    },
    {
      method: 'POST',
      path: '/api/teams',
      access: { kind: 'user' },
      limit: 'create',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const team = await inTransaction(pool, async (c) => {
          const { rows } = await c.query<{ id: string }>('INSERT INTO team (name) VALUES ($1) RETURNING id', [body.name]);
          await c.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'owner')`, [rows[0].id, ctx.user.id]);
          return rows[0];
        });
        return ok({ id: team.id, name: body.name, role: 'owner', projects: [] } satisfies TeamDto, 201);
      },
    },
    {
      method: 'PATCH',
      path: '/api/teams/:teamId',
      access: { kind: 'team', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        await pool.query('UPDATE team SET name = $2 WHERE id = $1', [ctx.teamId, body.name]);
        return ok({ ok: true });
      },
    },
    {
      method: 'DELETE',
      path: '/api/teams/:teamId',
      access: { kind: 'team', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        // Сначала проекты: случайное удаление команды не должно трогать файлы.
        const gone = await pool.query(
          'DELETE FROM team t WHERE t.id = $1 AND NOT EXISTS (SELECT 1 FROM project p WHERE p.team_id = t.id)',
          [ctx.teamId],
        );
        return gone.rowCount === 0 ? apiError(409, 'NOT_EMPTY') : ok({ ok: true });
      },
    },
    {
      method: 'GET',
      path: '/api/teams/:teamId/members',
      access: { kind: 'team', min: 'member' },
      limit: 'read',
      handle: async (ctx) => ok(await members(deps, ctx.teamId)),
    },
    {
      method: 'POST',
      path: '/api/teams/:teamId/invite',
      access: { kind: 'team', min: 'owner' },
      limit: 'mail',
      handle: async (ctx) => {
        const body = parseTeamInvite(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<{ id: string }>('SELECT id FROM "user" WHERE email = $1', [body.email]);
        if (rows[0]) {
          await pool.query(
            `INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'member') ON CONFLICT (team_id, user_id) DO NOTHING`,
            [ctx.teamId, rows[0].id],
          );
        } else {
          await pool.query(
            'INSERT INTO team_invite (team_id, email, invited_by) VALUES ($1, $2, $3) ON CONFLICT (team_id, email) DO NOTHING',
            [ctx.teamId, body.email, ctx.user.id],
          );
        }
        const name = await pool.query<{ name: string }>('SELECT name FROM team WHERE id = $1', [ctx.teamId]);
        sendInvite(
          deps,
          body.email,
          teamInviteMail(body.email, ctx.user.name || ctx.user.email, name.rows[0].name, `${deps.publicUrl}/#/`),
        );
        return ok(await members(deps, ctx.teamId));
      },
    },
    {
      method: 'DELETE',
      path: '/api/teams/:teamId/members/:entryId',
      access: { kind: 'team', min: 'member' },
      limit: 'write',
      handle: async (ctx: Ctx) => {
        const entry = ctx.params.entryId;
        const invite = await pool.query('SELECT 1 FROM team_invite WHERE id = $1 AND team_id = $2', [entry, ctx.teamId]);
        if (invite.rowCount) {
          if (ctx.teamRole !== 'owner') return apiError(403, 'FORBIDDEN');
          await pool.query('DELETE FROM team_invite WHERE id = $1', [entry]);
          return ok(await members(deps, ctx.teamId));
        }
        const { rows } = await pool.query<{ user_id: string; role: string }>(
          'SELECT user_id, role FROM team_member WHERE id = $1 AND team_id = $2',
          [entry, ctx.teamId],
        );
        const target = rows[0];
        if (!target) return apiError(404, 'NOT_FOUND');
        // Участник может уйти сам; владельца снять нельзя: команда осталась бы без него.
        if (target.role === 'owner' || (ctx.teamRole !== 'owner' && target.user_id !== ctx.user.id)) return apiError(403, 'FORBIDDEN');
        await inTransaction(pool, async (c) => {
          const owner = await c.query<{ user_id: string }>(
            `SELECT user_id FROM team_member WHERE team_id = $1 AND role = 'owner' LIMIT 1`,
            [ctx.teamId],
          );
          await handOverFiles(c, ctx.teamId, target.user_id, owner.rows[0].user_id);
          await c.query('DELETE FROM team_member WHERE id = $1', [entry]);
        });
        return ok(await members(deps, ctx.teamId));
      },
    },
    {
      method: 'POST',
      path: '/api/teams/:teamId/projects',
      access: { kind: 'team', min: 'member' },
      limit: 'create',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<{ id: string; team_id: string; name: string }>(
          'INSERT INTO project (team_id, name) VALUES ($1, $2) RETURNING id, team_id, name',
          [ctx.teamId, body.name],
        );
        return ok(projectDto(rows[0]), 201);
      },
    },
    {
      method: 'PATCH',
      path: '/api/projects/:projectId',
      access: { kind: 'project', min: 'member' },
      limit: 'write',
      handle: async (ctx) => {
        const body = parseTeamCreate(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<{ id: string; team_id: string; name: string }>(
          'UPDATE project SET name = $2 WHERE id = $1 RETURNING id, team_id, name',
          [ctx.params.projectId, body.name],
        );
        return ok(projectDto(rows[0]));
      },
    },
    {
      method: 'DELETE',
      path: '/api/projects/:projectId',
      access: { kind: 'project', min: 'owner' },
      limit: 'write',
      handle: async (ctx) => {
        const gone = await pool.query(
          `DELETE FROM project p WHERE p.id = $1 AND NOT EXISTS (SELECT 1 FROM file f WHERE f.project_id = p.id AND f.trashed_at IS NULL)`,
          [ctx.params.projectId],
        );
        return gone.rowCount === 0 ? apiError(409, 'NOT_EMPTY') : ok({ ok: true });
      },
    },
  ];
}
