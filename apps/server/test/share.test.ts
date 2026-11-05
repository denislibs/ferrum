import { afterAll, beforeEach, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import type { ApiDeps } from '../src/api';
import { INVITES_PER_ADDRESS, sendInvite } from '../src/invites';
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
  for (const key of (await s.valkey.send('KEYS', ['api:mail:*'])) as string[]) await s.valkey.del(key);
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

test('предел писем считается по ящику: варианты адреса отвергаются или делят счётчик', async () => {
  const f = await s.newFile(owner);
  // Варианты строки, которые дошли бы до того же ящика, адресом не считаются.
  for (const variant of ['x<box@example.com>', 'z,box@example.com', '(c)box@example.com', 'q:box@example.com;']) {
    const res = await invite(owner, f.id, variant, 'viewer');
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: 'BAD_REQUEST' });
  }
  expect((await share(owner, f.id)).people.map((p: { email: string }) => p.email)).toEqual(['owner@example.com']);
  for (const to of ['box@example.com', 'box+1@example.com', 'b.ox@example.com', 'box+2@example.com', 'box+3@example.com']) {
    expect((await invite(owner, f.id, to, 'viewer')).status).toBe(200);
  }
  expect((await invite(owner, f.id, 'BOX+4@example.com', 'viewer')).status).toBe(200);
  // Письмо уходит после ответа; счётчик дошёл до шести — решение о шестом принято.
  for (let i = 0; i < 500 && (await s.valkey.get('invite-to:box@example.com')) !== '6'; i++) await Bun.sleep(10);
  expect(await s.valkey.get('invite-to:box@example.com')).toBe('6');
  const boxed = () => s.mails.filter((m) => /^b\.?ox/.test(m.to));
  for (let i = 0; i < 500 && boxed().length < INVITES_PER_ADDRESS; i++) await Bun.sleep(10);
  expect(boxed()).toHaveLength(INVITES_PER_ADDRESS);
});

test('запрос доступа съел предел адреса владельца: письма нет, но повторить можно', async () => {
  const f = await s.newFile(owner);
  await s.valkey.set('invite-to:owner@example.com', String(INVITES_PER_ADDRESS));
  const ask = () => s.call(guest, 'POST', `/api/files/${f.id}/request-access`);
  expect((await ask()).status).toBe(200);
  const key = `request-access:${f.id}:${guest.id}`;
  for (let i = 0; i < 500 && (await s.valkey.get(key)) !== null; i++) await Bun.sleep(10);
  expect(await s.valkey.get(key)).toBeNull();
  await s.valkey.del('invite-to:owner@example.com');
  const seen = s.mails.filter((m) => m.to === owner.email).length;
  expect((await ask()).status).toBe(200);
  await s.waitMail(owner.email, seen);
  expect(s.mails.filter((m) => m.to === owner.email)).toHaveLength(seen + 1);
  // Письмо ушло — второго сутки нет.
  expect((await ask()).status).toBe(200);
  expect(await s.valkey.get(key)).toBe('2');
});

test('письмо уходит только на проверенный адрес', async () => {
  const sent: string[] = [];
  const deps = {
    valkey: s.valkey,
    mailer: { send: async (m: { to: string }) => void sent.push(m.to) },
    allowedEmails: [],
  } as unknown as ApiDeps;
  const mail = (to: string) => ({ to, subject: '', text: '', html: '' });
  sendInvite(deps, 'x<trap@example.com>', mail('x<trap@example.com>'));
  sendInvite(deps, 'clean@example.com', mail('clean@example.com'));
  for (let i = 0; i < 500 && sent.length === 0; i++) await Bun.sleep(10);
  expect(sent).toEqual(['clean@example.com']);
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

const newLink = async (id: string, linkRole = 'editor') =>
  (await json(await s.call(owner, 'PUT', `/api/files/${id}/share/link`, { enabled: true, role: linkRole }))).link.token as string;
const redeem = (who: Person, id: string, token: string) => s.call(who, 'POST', `/api/files/${id}/redeem`, { token });

test('пришедший только по ссылке: людей не видит и не зовёт, себе доступ не закрепить', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  const token = await newLink(f.id);
  expect((await redeem(guest, f.id, token)).status).toBe(200);
  expect(await role(guest, f.id)).toBe('editor');
  expect((await json(await s.call(guest, 'GET', `/api/files/${f.id}/access`))).canShare).toBe(false);
  // Редактировать может, это его роль по ссылке.
  expect((await s.writeContent(guest, f.id, 0)).status).toBe(200);

  const list = await s.call(guest, 'GET', `/api/files/${f.id}/share`);
  expect(list.status).toBe(403);
  const body = await list.text();
  expect(JSON.parse(body)).toEqual({ error: 'FORBIDDEN' });
  expect(body).not.toContain(owner.email);
  for (const email of [guest.email, 'friend@example.com']) {
    const res = await invite(guest, f.id, email, 'editor');
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: 'FORBIDDEN' });
  }
  expect((await s.pool.query('SELECT 1 FROM file_access WHERE file_id = $1 AND user_id = $2', [f.id, guest.id])).rowCount).toBe(0);
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(0);

  // Выключение и новая ссылка закрывают доступ целиком.
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: false, role: 'editor' });
  expect(await role(guest, f.id)).toBe(404);
  expect((await invite(guest, f.id, guest.email)).status).toBe(404);
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'editor' });
  expect(await role(guest, f.id)).toBe('editor');
  await s.call(owner, 'POST', `/api/files/${f.id}/share/link/reset`);
  expect(await role(guest, f.id)).toBe(404);
});

test('прямая роль и ссылка: действует большая, звать людей по ссылке нельзя, прямая переживает ссылку', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  const token = await newLink(f.id);
  await redeem(viewer, f.id, token);
  expect(await role(viewer, f.id)).toBe('editor');
  expect((await s.writeContent(viewer, f.id, 0)).status).toBe(200);
  // Редактор по ссылке и читатель сам по себе: адреса ему не положены.
  expect((await s.call(viewer, 'GET', `/api/files/${f.id}/share`)).status).toBe(403);
  expect((await invite(viewer, f.id, viewer.email, 'editor')).status).toBe(403);
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: false, role: 'editor' });
  expect(await role(viewer, f.id)).toBe('viewer');
  expect((await s.writeContent(viewer, f.id, 1)).status).toBe(403);
  // Прямой редактор с той же ссылкой людей видит: меньшая роль по ссылке ему не мешает.
  await invite(owner, f.id, editor.email, 'editor');
  await s.call(owner, 'PUT', `/api/files/${f.id}/share/link`, { enabled: true, role: 'viewer' });
  await redeem(editor, f.id, token);
  expect(await role(editor, f.id)).toBe('editor');
  expect((await s.call(editor, 'GET', `/api/files/${f.id}/share`)).status).toBe(200);
  expect((await json(await s.call(editor, 'GET', `/api/files/${f.id}/access`))).canShare).toBe(true);
});

test('файл в корзине по ссылке не даёт доступа и после возврата', async () => {
  const f = await s.newFile(owner);
  const token = await newLink(f.id, 'viewer');
  await s.call(owner, 'POST', `/api/files/${f.id}/trash`);
  expect((await redeem(guest, f.id, token)).status).toBe(404);
  await s.call(owner, 'POST', `/api/files/${f.id}/restore`);
  expect(await role(guest, f.id)).toBe(404);
});

test('новая ссылка — одна транзакция: сбой оставляет прежнюю ссылку и пришедших целиком', async () => {
  const f = await s.newFile(owner);
  const token = await newLink(f.id, 'viewer');
  await redeem(guest, f.id, token);
  const res = await s.failing('file_link_user', 'DELETE', () => s.call(owner, 'POST', `/api/files/${f.id}/share/link/reset`));
  expect(res.status).toBe(503);
  expect((await share(owner, f.id)).link.token).toBe(token);
  expect(await role(guest, f.id)).toBe('viewer');
});

test('снятие доступа — одна транзакция: сбой не оставляет человека без записи, но с приходом по ссылке', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, viewer.email, 'viewer');
  const token = await newLink(f.id, 'viewer');
  await redeem(viewer, f.id, token);
  const id = await entry(f.id, viewer.email);
  const res = await s.failing('file_link_user', 'DELETE', () => s.call(owner, 'DELETE', `/api/files/${f.id}/share/people/${id}`));
  expect(res.status).toBe(503);
  expect(await entry(f.id, viewer.email)).toBe(id);
  expect((await s.call(owner, 'DELETE', `/api/files/${f.id}/share/people/${id}`)).status).toBe(200);
  expect(await role(viewer, f.id)).toBe(404);
});

test('`redeem` не переживает новую ссылку: вставка ждёт её транзакцию и видит новый токен', async () => {
  const f = await s.newFile(owner);
  const token = await newLink(f.id, 'viewer');
  // Новая ссылка посреди транзакции: старый токен ещё виден, запись о пришедшем уже удалена.
  const reset = await s.pool.connect();
  await reset.query('BEGIN');
  await reset.query(`UPDATE share_link SET token_hash = 'новый', nonce = 'новый' WHERE file_id = $1`, [f.id]);
  await reset.query('DELETE FROM file_link_user WHERE file_id = $1', [f.id]);
  let finished = false;
  const pendingRedeem = redeem(guest, f.id, token).finally(() => (finished = true));
  const waiting = async () => {
    const { rowCount } = await s.pool.query(
      `SELECT 1 FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND query LIKE 'INSERT INTO file_link_user%'`,
    );
    return rowCount! > 0;
  };
  while (!finished && !(await waiting())) await Bun.sleep(5);
  await reset.query('COMMIT');
  reset.release();
  const res = await pendingRedeem;
  expect(res.status).toBe(404);
  expect((await s.pool.query('SELECT 1 FROM file_link_user')).rowCount).toBe(0);
});

test('приглашение не теряется при сбое первого входа: перейдёт при следующем', async () => {
  const f = await s.newFile(owner);
  await invite(owner, f.id, 'unlucky@example.com', 'editor');
  const fresh = await s.failing('file_access', 'INSERT', () => s.signIn('unlucky@example.com'));
  expect(await role(fresh, f.id)).toBe(404);
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(1);
  const again = await s.signIn('unlucky@example.com');
  expect(await role(again, f.id)).toBe('editor');
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(0);
});

test('приглашение из гонки с первым входом: запись на существующий адрес становится доступом при новом входе', async () => {
  const f = await s.newFile(owner);
  await s.pool.query(`INSERT INTO file_invite (file_id, email, role, invited_by) VALUES ($1, $2, 'viewer', $3)`, [
    f.id,
    guest.email,
    owner.id,
  ]);
  expect(await role(guest, f.id)).toBe(404);
  const again = await s.signIn(guest.email);
  expect(await role(again, f.id)).toBe('viewer');
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(0);
});

test('повторное приглашение существующего убирает запись из гонки: в списке одна строка', async () => {
  const f = await s.newFile(owner);
  await s.pool.query(`INSERT INTO file_invite (file_id, email, role, invited_by) VALUES ($1, $2, 'viewer', $3)`, [
    f.id,
    guest.email,
    owner.id,
  ]);
  const { people } = await json(await invite(owner, f.id, guest.email, 'viewer'));
  expect((people as { email: string }[]).filter((p) => p.email === guest.email)).toHaveLength(1);
  expect((await s.pool.query('SELECT 1 FROM file_invite')).rowCount).toBe(0);
  expect(await role(guest, f.id)).toBe('viewer');
});
