import { afterAll, beforeEach, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { INVITES_PER_ADDRESS } from '../src/invites';
import { apiStand, container, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('owner@example.com', 'Анна');
const editor = await s.signIn('editor@example.com', 'Борис');
const viewer = await s.signIn('viewer@example.com', 'Вера');
const guest = await s.signIn('guest@example.com', 'Гость');
beforeEach(async () => {
  await s.reset();
  // Счётчики писем на адрес: тесты не должны делить их между собой.
  for (const key of (await s.valkey.send('KEYS', ['invite-to:*'])) as string[]) await s.valkey.del(key);
  for (const key of (await s.valkey.send('KEYS', ['request-access:*'])) as string[]) await s.valkey.del(key);
});

const json = async (res: Response) => (await res.json()) as any; // eslint-disable-line @typescript-eslint/no-explicit-any
const share = async (who: Person, id: string) => json(await s.call(who, 'GET', `/api/files/${id}/share`));
const invite = (who: Person, id: string, email: string, role = 'editor') =>
  s.call(who, 'POST', `/api/files/${id}/share/invite`, { email, role });
const role = async (who: Person, id: string) => {
  const res = await s.call(who, 'GET', `/api/files/${id}/access`);
  return res.status === 200 ? (await json(res)).role : res.status;
};
const entry = async (id: string, email: string) => {
  const { people } = await share(owner, id);
  return (people as { id: string; email: string }[]).find((p) => p.email === email)!.id;
};

test('приглашение существующего: роль, письмо со ссылкой на файл, файл в «Доступные мне»', async () => {
  const f = await s.newFile(owner, 'Макет');
  const res = await invite(owner, f.id, editor.email);
  expect(res.status).toBe(200);
  expect(await role(editor, f.id)).toBe('editor');
  const mail = await s.waitMail(editor.email, 1);
  expect(mail.text).toContain(`${s.publicUrl}/#/file/${f.id}`);
  expect(mail.text).toContain('«Макет»');
  const shared = await json(await s.call(editor, 'GET', '/api/files?section=shared&start=0&count=10'));
  expect(shared.rows.map((r: { id: string }) => r.id)).toEqual([f.id]);
  expect(shared.rows[0].role).toBe('editor');
});

test('приглашение нового адреса: при первом входе файл доступен; список не выдаёт, есть ли учётная запись', async () => {
  const f = await s.newFile(owner);
  const newbie = await json(await invite(owner, f.id, 'newbie@example.com', 'viewer'));
  const known = await json(await invite(owner, f.id, editor.email, 'viewer'));
  const row = (list: { people: { email: string }[] }, email: string) => list.people.find((p) => p.email === email)!;
  // Обе записи одного вида: имени нет, пока человек не открывал файл.
  expect(Object.keys(row(newbie, 'newbie@example.com')).sort()).toEqual(['email', 'id', 'name', 'role']);
  expect(row(known, 'newbie@example.com')).toMatchObject({ name: null, role: 'viewer' });
  expect(row(known, editor.email)).toMatchObject({ name: null, role: 'viewer' });
  await s.call(editor, 'POST', `/api/files/${f.id}/touch`);
  expect((await share(owner, f.id)).people.find((p: { email: string }) => p.email === editor.email).name).toBe('Борис');
  const fresh = await s.signIn('newbie@example.com');
  expect(await role(fresh, f.id)).toBe('viewer');
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(0);
});

test('смена и снятие роли действуют сразу, в том числе на запись', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, editor.email);
  expect((await s.writeContent(editor, f.id, 0)).status).toBe(200);
  const id = await entry(f.id, editor.email);
  await s.call(owner, 'PATCH', `/api/files/${f.id}/share/people/${id}`, { role: 'viewer' });
  expect(await role(editor, f.id)).toBe('viewer');
  expect((await s.writeContent(editor, f.id, 1)).status).toBe(403);
  await s.call(owner, 'DELETE', `/api/files/${f.id}/share/people/${id}`);
  expect(await role(editor, f.id)).toBe(404);
  expect((await s.writeContent(editor, f.id, 1)).status).toBe(404);
  expect((await s.call(editor, 'GET', `/api/files/${f.id}/content`)).status).toBe(404);
});

test('запись другого файла по его id не меняется и не удаляется (IDOR)', async () => {
  const mine = await s.newFile(guest);
  const theirs = await s.newFile(owner);
  await invite(owner, theirs.id, viewer.email, 'viewer');
  const foreign = await entry(theirs.id, viewer.email);
  expect((await s.call(guest, 'PATCH', `/api/files/${mine.id}/share/people/${foreign}`, { role: 'editor' })).status).toBe(404);
  expect((await s.call(guest, 'DELETE', `/api/files/${mine.id}/share/people/${foreign}`)).status).toBe(404);
  expect(await role(viewer, theirs.id)).toBe('viewer');
});

test('владельца нельзя понизить и снять; редактор не меняет чужие роли, но может добавить', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, editor.email);
  const self = await entry(f.id, owner.email);
  expect((await s.call(owner, 'PATCH', `/api/files/${f.id}/share/people/${self}`, { role: 'viewer' })).status).toBe(403);
  expect((await s.call(owner, 'DELETE', `/api/files/${f.id}/share/people/${self}`)).status).toBe(403);
  expect(await role(owner, f.id)).toBe('owner');
  // Повторное приглашение от редактора не понижает: и владельца, и другого редактора.
  await invite(owner, f.id, guest.email, 'editor');
  await invite(editor, f.id, owner.email, 'viewer');
  await invite(editor, f.id, guest.email, 'viewer');
  expect(await role(owner, f.id)).toBe('owner');
  expect(await role(guest, f.id)).toBe('editor');
  await invite(editor, f.id, viewer.email, 'viewer');
  expect(await role(viewer, f.id)).toBe('viewer');
  await invite(editor, f.id, viewer.email, 'editor');
  expect(await role(viewer, f.id)).toBe('editor');
});

test('читатель сам снимает свой доступ, чужой — нет', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  await invite(owner, f.id, guest.email, 'viewer');
  const mine = await entry(f.id, viewer.email);
  const theirs = await entry(f.id, guest.email);
  expect((await s.call(viewer, 'DELETE', `/api/files/${f.id}/share/people/${theirs}`)).status).toBe(403);
  expect((await s.call(viewer, 'DELETE', `/api/files/${f.id}/share/people/${mine}`)).status).toBe(200);
  expect(await role(viewer, f.id)).toBe(404);
});

test('ссылка: токен в базе хэшем, роль по ссылке, выключатель действует сразу', async () => {
  const f = await s.newFile(owner);
  const on = await json(await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' }));
  const token = on.link.token as string;
  expect(token).toHaveLength(43);
  const { rows } = await s.pool.query('SELECT * FROM share_link');
  expect(JSON.stringify(rows[0])).not.toContain(token);
  expect(rows[0].token_hash).toBe(createHash('sha256').update(token).digest('hex'));
  // Показать ссылку снова можно, токен тот же.
  expect((await share(owner, f.id)).link.token).toBe(token);
  // Редактор ссылки не видит.
  await invite(owner, f.id, editor.email);
  expect((await share(editor, f.id)).link).toBeNull();

  expect(await role(guest, f.id)).toBe(404);
  const redeem = await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token });
  expect(redeem.status).toBe(200);
  expect((await json(redeem)).role).toBe('viewer');
  expect(await role(guest, f.id)).toBe('viewer');
  expect((await s.call(guest, 'GET', `/api/files?section=shared&start=0&count=10`)).status).toBe(200);
  expect((await json(await s.call(guest, 'GET', `/api/files?section=shared&start=0&count=10`))).total).toBe(1);

  // Роль ссылки меняется для всех, кто по ней пришёл; прямая роль важнее меньшей.
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'editor' });
  expect(await role(guest, f.id)).toBe('editor');
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: false, role: 'editor' });
  expect(await role(guest, f.id)).toBe(404);
  expect((await json(await s.call(guest, 'GET', `/api/files?section=shared&start=0&count=10`))).total).toBe(0);
  // Выключенная ссылка не открывает файл, токен прежний после включения работает.
  expect((await s.call(viewer, 'POST', `/api/files/${f.id}/redeem`, { token })).status).toBe(404);
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' });
  expect((await share(owner, f.id)).link.token).toBe(token);
  expect(await role(guest, f.id)).toBe('viewer');
});

test('новая ссылка отменяет старую и всех пришедших по ней', async () => {
  const f = await s.newFile(owner);
  const old = (await json(await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' }))).link.token;
  await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token: old });
  const next = (await json(await s.call(owner, 'POST', `/api/files/${f.id}/share/link/reset`))).link.token;
  expect(next).not.toBe(old);
  expect(await role(guest, f.id)).toBe(404);
  expect((await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token: old })).status).toBe(404);
  expect((await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token: next })).status).toBe(200);
});

test('неверный токен, файл без ссылки и чужой токен неотличимы от несуществующего файла', async () => {
  const a = await s.newFile(owner);
  const b = await s.newFile(owner);
  const tokenB = (await json(await s.call(owner, 'PUT', `/api/files/${b.id}/share/link`, { enabled: true, role: 'editor' }))).link.token;
  const bad = 'y'.repeat(43);
  const missing = await s.call(guest, 'POST', `/api/files/00000000-0000-4000-8000-000000000000/redeem`, { token: bad });
  const noLink = await s.call(guest, 'POST', `/api/files/${a.id}/redeem`, { token: bad });
  const wrong = await s.call(guest, 'POST', `/api/files/${b.id}/redeem`, { token: bad });
  const foreign = await s.call(guest, 'POST', `/api/files/${a.id}/redeem`, { token: tokenB });
  for (const res of [missing, noLink, wrong, foreign]) {
    expect(res.status).toBe(404);
    expect(await res.json()).toEqual({ error: 'NOT_FOUND' });
  }
  expect((await s.call(guest, 'POST', `/api/files/${a.id}/redeem`, { token: '../x' })).status).toBe(400);
});

test('файл из корзины по ссылке не открывается', async () => {
  const f = await s.newFile(owner);
  const token = (await json(await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'editor' }))).link.token;
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await s.call(guest, 'POST', `/api/files/${f.id}/redeem`, { token })).status).toBe(404);
  expect((await s.call(guest, 'GET', `/api/files/${f.id}`)).status).toBe(404);
});

test('запрос доступа: письмо владельцу один раз, ответ один и тот же для чужого и несуществующего', async () => {
  const f = await s.newFile(owner, 'Секрет');
  const seen = s.mails.filter((m) => m.to === owner.email).length;
  const a = await s.call(guest, 'POST', `/api/files/${f.id}/request-access`);
  const b = await s.call(guest, 'POST', '/api/files/00000000-0000-4000-8000-000000000000/request-access');
  expect([a.status, await a.text()]).toEqual([b.status, await b.text()]);
  const mail = await s.waitMail(owner.email, seen);
  expect(mail.subject).toBe('Запрос доступа к файлу в Ferrum');
  expect(mail.text).toContain(guest.email === 'guest@example.com' ? 'Гость' : '');
  await s.call(guest, 'POST', `/api/files/${f.id}/request-access`);
  await Bun.sleep(30);
  expect(s.mails.filter((m) => m.to === owner.email)).toHaveLength(seen + 1);
});

test('письма приглашений на один адрес ограничены, запись при этом остаётся', async () => {
  const f = await s.newFile(owner);
  for (let i = 0; i <= INVITES_PER_ADDRESS + 1; i++) {
    expect((await invite(owner, f.id, 'many@example.com', 'viewer')).status).toBe(200);
  }
  await s.waitMail('many@example.com', INVITES_PER_ADDRESS - 1);
  await Bun.sleep(30);
  expect(s.mails.filter((m) => m.to === 'many@example.com')).toHaveLength(INVITES_PER_ADDRESS);
});

test('адрес вне `ALLOWED_EMAILS`: ответ тот же, письма нет', async () => {
  const closed = await apiStand({ allowedEmails: ['example.org'] }, true);
  // Вход в этом стенде открыт (`allowedEmails` — только у API), поэтому здесь адрес вне списка API.
  const who = await closed.signIn('boss@example.com');
  const f = await closed.newFile(who);
  expect((await closed.call(who, 'POST', `/api/files/${f.id}/share/invite`, { email: 'x@example.com', role: 'viewer' })).status).toBe(200);
  await Bun.sleep(30);
  expect(closed.mails.filter((m) => m.to === 'x@example.com')).toHaveLength(0);
  await closed.stop();
});

test('роль после смены видна и через запись содержимого читателем', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  expect((await s.writeContent(viewer, f.id, 0, container())).status).toBe(403);
});
