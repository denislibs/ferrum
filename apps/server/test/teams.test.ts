import { afterAll, beforeEach, expect, test } from 'bun:test';
import { apiStand, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com');
const member = await s.signIn('member@example.com');
const outsider = await s.signIn('outsider@example.com');
beforeEach(s.reset);

const json = async (res: Response) => (await res.json()) as any; // eslint-disable-line @typescript-eslint/no-explicit-any
const newTeam = async (who: Person, name = 'Команда') => (await json(await s.call(who, 'POST', '/api/teams', { name }))) as { id: string };
const newProject = async (who: Person, team: string, name = 'Проект') =>
  (await json(await s.call(who, 'POST', `/api/teams/${team}/projects`, { name }))) as { id: string };
const invite = (who: Person, team: string, email: string) => s.call(who, 'POST', `/api/teams/${team}/invite`, { email });
const role = async (who: Person, file: string) => json(await s.call(who, 'GET', `/api/files/${file}/access`));
const entries = async (who: Person, team: string) =>
  (await json(await s.call(who, 'GET', `/api/teams/${team}/members`))).members as { id: string; email: string }[];
const list = async (who: Person, section: string) => json(await s.call(who, 'GET', `/api/files?section=${section}&start=0&count=50`));

test('команда: создатель — владелец, проект виден участнику, не виден чужому', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  expect((await json(await s.call(owner, 'GET', '/api/teams'))).teams).toEqual([
    { id: t.id, name: 'Команда', role: 'owner', projects: [{ id: p.id, teamId: t.id, name: 'Проект' }] },
  ]);
  expect((await json(await s.call(outsider, 'GET', '/api/teams'))).teams).toEqual([]);
  await invite(owner, t.id, member.email);
  expect((await json(await s.call(member, 'GET', '/api/teams'))).teams[0].role).toBe('member');
});

test('участник — редактор файлов проекта, владелец команды — владелец, чужой — ничего', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  await invite(owner, t.id, member.email);
  const f = await s.newFile(member, 'В проекте', p.id);
  const g = await s.newFile(owner, 'Ещё', p.id);
  expect((await role(member, g.id)).role).toBe('editor');
  expect((await role(owner, f.id)).role).toBe('owner');
  expect((await s.call(outsider, 'GET', `/api/files/${f.id}`)).status).toBe(404);
  expect((await list(member, `project:${p.id}`)).total).toBe(2);
  expect((await list(outsider, `project:${p.id}`)).total).toBe(0);
  // Файл проекта не в «Черновиках» создателя.
  expect((await list(member, 'drafts')).total).toBe(0);
  // Чужой проект при создании неотличим от несуществующего.
  expect((await s.call(outsider, 'POST', '/api/files', { name: 'x', projectId: p.id })).status).toBe(404);
});

test('действующая роль — наибольшая из прямой и через команду', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  const f = await s.newFile(owner, 'Файл', p.id);
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f.id, member.id]);
  expect((await role(member, f.id)).role).toBe('viewer');
  await invite(owner, t.id, member.email);
  expect((await role(member, f.id)).role).toBe('editor');
});

test('перемещение: только в свою команду, из черновиков в проект и обратно', async () => {
  const f = await s.newFile(owner);
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  const foreign = await newProject(outsider, (await newTeam(outsider)).id);
  expect((await s.call(owner, 'POST', `/api/files/${f.id}/move`, { projectId: foreign.id })).status).toBe(404);
  expect((await json(await s.call(owner, 'POST', `/api/files/${f.id}/move`, { projectId: p.id }))).projectId).toBe(p.id);
  expect((await list(owner, 'drafts')).total).toBe(0);
  expect((await s.call(owner, 'POST', `/api/files/${f.id}/move`, { projectId: null })).status).toBe(200);
  expect((await list(owner, 'drafts')).total).toBe(1);
});

test('приглашение адреса без учётной записи: при первом входе он в команде', async () => {
  const t = await newTeam(owner);
  await newProject(owner, t.id);
  const res = await invite(owner, t.id, 'Fresh@Example.com');
  expect(res.status).toBe(200);
  expect((await json(res)).members.map((m: { email: string }) => m.email)).toContain('fresh@example.com');
  const mail = await s.waitMail('fresh@example.com');
  expect(mail.subject).toBe('Вас пригласили в команду в Ferrum');
  expect(mail.text).toContain('«Команда»');
  const fresh = await s.signIn('fresh@example.com');
  expect((await json(await s.call(fresh, 'GET', '/api/teams'))).teams[0].id).toBe(t.id);
  expect((await s.pool.query('SELECT 1 FROM team_invite')).rowCount).toBe(0);
});

test('имя с разметкой в письме экранируется', async () => {
  const t = await newTeam(owner, '<b>Команда</b>');
  await invite(owner, t.id, 'html@example.com');
  const mail = await s.waitMail('html@example.com');
  expect(mail.html).toContain('&lt;b&gt;Команда&lt;/b&gt;');
  expect(mail.html).not.toContain('<b>');
});

test('ушедший участник теряет доступ, его файлы переходят владельцу команды', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  await invite(owner, t.id, member.email);
  const f = await s.newFile(member, 'Его файл', p.id);
  const entry = (await entries(owner, t.id)).find((m) => m.email === member.email)!;
  expect((await s.call(member, 'DELETE', `/api/teams/${t.id}/members/${entry.id}`)).status).toBe(200);
  expect((await s.call(member, 'GET', `/api/files/${f.id}`)).status).toBe(404);
  expect((await role(owner, f.id)).role).toBe('owner');
  const { rows } = await s.pool.query('SELECT owner_id FROM file WHERE id = $1', [f.id]);
  expect(rows[0].owner_id).toBe(owner.id);
  expect((await list(owner, `project:${p.id}`)).total).toBe(1);
});

test('владельца снять нельзя, другого участника может только владелец', async () => {
  const t = await newTeam(owner);
  await invite(owner, t.id, member.email);
  await invite(owner, t.id, outsider.email);
  const all = await entries(owner, t.id);
  const ownerEntry = all.find((m) => m.email === owner.email)!;
  const outsiderEntry = all.find((m) => m.email === outsider.email)!;
  expect((await s.call(owner, 'DELETE', `/api/teams/${t.id}/members/${ownerEntry.id}`)).status).toBe(403);
  expect((await s.call(member, 'DELETE', `/api/teams/${t.id}/members/${outsiderEntry.id}`)).status).toBe(403);
  // Запись чужой команды по чужому адресу — 404.
  const other = await newTeam(member);
  expect((await s.call(member, 'DELETE', `/api/teams/${other.id}/members/${outsiderEntry.id}`)).status).toBe(404);
});

test('нельзя удалить команду с проектами и проект с файлами; файл из корзины уходит в черновики', async () => {
  const t = await newTeam(owner);
  const p = await newProject(owner, t.id);
  const f = await s.newFile(owner, 'Файл', p.id);
  const refused = await s.call(owner, 'DELETE', `/api/projects/${p.id}`);
  expect(refused.status).toBe(409);
  expect(await refused.json()).toEqual({ error: 'NOT_EMPTY' });
  expect((await s.call(owner, 'DELETE', `/api/teams/${t.id}`)).status).toBe(409);
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await s.call(owner, 'DELETE', `/api/projects/${p.id}`)).status).toBe(200);
  await s.call(owner, 'POST', `/api/files/${f.id}/restore`);
  expect((await list(owner, 'drafts')).total).toBe(1);
  expect((await s.call(owner, 'DELETE', `/api/teams/${t.id}`)).status).toBe(200);
});

test('пустое и управляющие символы в именах — 400', async () => {
  const t = await newTeam(owner);
  for (const name of ['', '   ', 'a\nb', 'x'.repeat(101)]) {
    expect((await s.call(owner, 'POST', '/api/teams', { name })).status).toBe(400);
    expect((await s.call(owner, 'POST', `/api/teams/${t.id}/projects`, { name })).status).toBe(400);
    expect((await s.call(owner, 'PATCH', `/api/teams/${t.id}`, { name })).status).toBe(400);
  }
  expect((await s.call(owner, 'POST', `/api/teams/${t.id}/invite`, { email: 'нет' })).status).toBe(400);
});
