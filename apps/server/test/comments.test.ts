// Комментарии через REST (спека, раздел 10): права каждого действия, чужие id, текст, упоминания, прочтение, номер и
// `?after=`, пределы, удаление файла и человека, очередь писем. Ответы сервера разбирает строгий разбор клиента: что не
// примет клиент, не пропускает и тест.

import {
  COMMENT_BODY_MAX,
  COMMENT_PAGE_MAX,
  COMMENT_PREVIEW_MAX,
  COMMENT_THREADS_MAX,
  COMMENT_THREADS_PER_USER_MAX,
  commentLength,
  commentPreview,
  parseCommentList,
  parseCommentWrite,
  parseMentionList,
  parseThreadDetail,
  THREAD_BYTES_MAX,
  THREAD_COMMENTS_MAX,
  type CommentListDto,
  type CommentWriteDto,
  type PinDto,
  type ThreadDetailDto,
} from '@ferrum/protocol';
import { afterAll, beforeEach, expect, spyOn, test } from 'bun:test';
import { UserSlots } from '../src/api';
import { apiStand, type Person } from './api-stand';

const s = await apiStand();
afterAll(s.stop);
const owner = await s.signIn('c-owner@example.com', 'Ольга');
const editor = await s.signIn('c-editor@example.com', 'Эдуард');
const viewer = await s.signIn('c-viewer@example.com', 'Вера');
const linked = await s.signIn('c-linked@example.com', 'Лев');
const member = await s.signIn('c-member@example.com', 'Мария');
const stranger = await s.signIn('c-stranger@example.com', 'Чужой');

beforeEach(async () => {
  await s.reset();
  // Пределы частоты — на человека и общие для всех тестов файла: каждый тест начинает с нуля.
  for (const key of (await s.valkey.send('KEYS', ['api:*'])) as string[]) await s.valkey.del(key);
});

const pin: PinDto = { page: '3@7910247476534078275', node: '12@7910247476534078275', x: 10, y: 20, wx: 110, wy: 220 };

/** Файл владельца: редактор и читатель напрямую, «пришедший» — по ссылке, участник — через команду проекта. */
async function world(): Promise<{ file: string; other: string }> {
  const team = await s.pool.query<{ id: string }>(`INSERT INTO team (name) VALUES ('Команда') RETURNING id`);
  await s.pool.query(`INSERT INTO team_member (team_id, user_id, role) VALUES ($1, $2, 'owner'), ($1, $3, 'member')`, [
    team.rows[0].id,
    owner.id,
    member.id,
  ]);
  const project = await s.pool.query<{ id: string }>(`INSERT INTO project (team_id, name) VALUES ($1, 'Проект') RETURNING id`, [
    team.rows[0].id,
  ]);
  const file = (await s.newFile(owner, 'Файл', project.rows[0].id)).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
  await s.pool.query(`INSERT INTO share_link (file_id, token_hash, nonce, role, enabled) VALUES ($1, $2, 'n', 'viewer', true)`, [
    file,
    'c'.repeat(64),
  ]);
  await s.pool.query('INSERT INTO file_link_user (file_id, user_id) VALUES ($1, $2)', [file, linked.id]);
  // Все уже открывали файл: приглашённого, не открывавшего его, подсказка не предлагает и письма ему не идут (M3, L1).
  for (const p of [editor, viewer, linked, member]) await opened(p, file);
  // Другой файл того же владельца, где у всех тех же людей тоже есть доступ: чужие id проверяются не только доступом.
  const other = (await s.newFile(owner, 'Другой')).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    other,
    editor.id,
    viewer.id,
  ]);
  return { file, other };
}

const base = (file: string) => `/api/files/${file}/comments`;
/** Человек открывал файл (`file_recent`) — час назад. */
const opened = (who: Person, file: string) =>
  s.pool.query(`INSERT INTO file_recent (user_id, file_id, opened_at) VALUES ($1, $2, now() - interval '1 hour') ON CONFLICT DO NOTHING`, [
    who.id,
    file,
  ]);

async function write(res: Response, status = 201): Promise<CommentWriteDto> {
  const text = await res.text();
  expect(res.status, text).toBe(status);
  const dto = parseCommentWrite(JSON.parse(text));
  if (!dto) throw new Error(`ответ не разобран клиентом: ${text}`);
  return dto;
}

const create = async (who: Person, file: string, body = 'Привет', p: PinDto = pin) =>
  write(await s.call(who, 'POST', base(file), { pin: p, body }));
const reply = async (who: Person, file: string, thread: string, body = 'Ответ') =>
  write(await s.call(who, 'POST', `${base(file)}/${thread}/comments`, { body }));

async function list(who: Person, file: string, query = ''): Promise<CommentListDto> {
  const res = await s.call(who, 'GET', `${base(file)}${query}`);
  const text = await res.text();
  expect(res.status, text).toBe(200);
  const dto = parseCommentList(JSON.parse(text));
  if (!dto) throw new Error(`список не разобран клиентом: ${text}`);
  return dto;
}

async function detail(who: Person, file: string, thread: string): Promise<ThreadDetailDto> {
  const res = await s.call(who, 'GET', `${base(file)}/${thread}`);
  const text = await res.text();
  expect(res.status, text).toBe(200);
  const dto = parseThreadDetail(JSON.parse(text));
  if (!dto) throw new Error(`обсуждение не разобрано клиентом: ${text}`);
  return dto;
}

const rows = async (sql: string, params: unknown[] = []) => (await s.pool.query(sql, params)).rows;

test('обсуждение: создание, ответ, список и обсуждение целиком; читатель и пришедший по ссылке тоже пишут', async () => {
  const { file } = await world();
  const a = await create(viewer, file, 'Первый');
  expect(a.thread.first.body).toBe('Первый');
  expect(a.thread.author).toEqual({ id: viewer.id, name: 'Вера' });
  expect(a.thread.pin).toEqual(pin);
  expect(a.thread.replies).toBe(0);
  expect(a.thread.unread).toBe(false);
  expect(a.comment?.id).toBe(a.thread.first.id);
  const b = await reply(linked, file, a.thread.id, 'По ссылке');
  expect(b.seq).toBe(a.seq + 1);
  expect(b.thread.replies).toBe(1);
  expect(b.thread.participants).toEqual([viewer.id, linked.id]);
  expect(b.thread.lastAt).toBe(b.comment!.createdAt);
  const l = await list(owner, file);
  expect(l.seq).toBe(b.seq);
  expect(l.threads.map((t) => [t.id, t.seq, t.replies])).toEqual([[a.thread.id, b.seq, 1]]);
  const d = await detail(member, file, a.thread.id);
  expect(d.comments.map((c) => [c.body, c.author.name])).toEqual([
    ['Первый', 'Вера'],
    ['По ссылке', 'Лев'],
  ]);
  // Без пина — неверно.
  expect((await s.call(owner, 'POST', base(file), { body: 'x' })).status).toBe(400);
});

test('чужие id: обсуждение и комментарий другого файла через этот адрес — как несуществующие, без записи', async () => {
  const { file, other } = await world();
  const mine = await create(owner, file, 'Здесь');
  const theirs = await create(owner, other, 'Там');
  const reply2 = await reply(owner, other, theirs.thread.id);
  const t = theirs.thread.id;
  const c = reply2.comment!.id;
  const before = await rows('SELECT seq FROM comment_file ORDER BY file_id');
  const attempts: [string, string, unknown?][] = [
    ['GET', `${base(file)}/${t}`],
    ['POST', `${base(file)}/${t}/comments`, { body: 'x' }],
    ['PATCH', `${base(file)}/${t}`, { resolved: true }],
    ['PATCH', `${base(file)}/${t}`, { pin }],
    ['PATCH', `${base(file)}/${t}/comments/${c}`, { body: 'x' }],
    ['DELETE', `${base(file)}/${t}/comments/${c}`],
    ['DELETE', `${base(file)}/${t}/comments/${theirs.thread.first.id}`],
    ['POST', `${base(file)}/${t}/read`],
    // Своё обсуждение, чужой комментарий: тоже нет.
    ['PATCH', `${base(file)}/${mine.thread.id}/comments/${c}`, { body: 'x' }],
    ['DELETE', `${base(file)}/${mine.thread.id}/comments/${c}`],
  ];
  for (const [method, path, body] of attempts) {
    const res = await s.call(owner, method, path, body);
    expect(res.status, `${method} ${path}`).toBe(404);
    expect(await res.json()).toEqual({ error: 'NOT_FOUND' });
  }
  expect(await rows('SELECT seq FROM comment_file ORDER BY file_id')).toEqual(before);
  expect((await detail(owner, other, t)).comments.map((x) => x.body)).toEqual(['Там', 'Ответ']);
  expect(await rows('SELECT count(*)::int AS n FROM comment_read WHERE thread_id = $1 AND user_id <> $2', [t, owner.id])).toEqual([
    { n: 0 },
  ]);
  // Неверный вид id — 404 маршрута, до базы.
  expect((await s.call(owner, 'GET', `${base(file)}/not-a-uuid`)).status).toBe(404);
});

test('права: правка — только своё; удаление — своё или владелец; закрытие — автор или редактор; перенос — автор или владелец', async () => {
  const { file } = await world();
  const v = await create(viewer, file, 'Читатель');
  const r = await reply(editor, file, v.thread.id, 'Редактор');
  const path = (c: string) => `${base(file)}/${v.thread.id}/comments/${c}`;
  // Правка чужого — 403 и владельцу.
  for (const who of [owner, editor, linked])
    expect((await s.call(who, 'PATCH', path(v.comment!.id), { body: 'x' })).status, who.email).toBe(403);
  const edited = await write(await s.call(viewer, 'PATCH', path(v.comment!.id), { body: 'Правка' }), 200);
  expect(edited.comment?.body).toBe('Правка');
  expect(edited.comment?.editedAt).not.toBeNull();
  // Удаление чужого: читатель и редактор — нет, владелец — да.
  expect((await s.call(viewer, 'DELETE', path(r.comment!.id))).status).toBe(403);
  expect((await s.call(member, 'DELETE', path(r.comment!.id))).status).toBe(403);
  const del = await write(await s.call(owner, 'DELETE', path(r.comment!.id)), 200);
  expect(del.removed).toBe(false);
  expect(del.thread.replies).toBe(0);
  // Своё удаляет каждый.
  const mine = await reply(linked, file, v.thread.id, 'Моё');
  expect((await write(await s.call(linked, 'DELETE', path(mine.comment!.id)), 200)).thread.replies).toBe(0);
  // Закрыть: чужое обсуждение читателю и пришедшему по ссылке — нет; автору-читателю — да; редактору — да.
  const thread = `${base(file)}/${v.thread.id}`;
  expect((await s.call(linked, 'PATCH', thread, { resolved: true })).status).toBe(403);
  const resolved = await write(await s.call(viewer, 'PATCH', thread, { resolved: true }), 200);
  expect(resolved.thread.resolvedBy).toEqual({ id: viewer.id, name: 'Вера' });
  expect(resolved.thread.resolvedAt).not.toBeNull();
  const reopened = await write(await s.call(editor, 'PATCH', thread, { resolved: false }), 200);
  expect([reopened.thread.resolvedAt, reopened.thread.resolvedBy]).toEqual([null, null]);
  // Перенос: автор и владелец; редактор — нет.
  const moved = { ...pin, node: null, x: 5, y: 6, wx: 5, wy: 6 };
  expect((await s.call(editor, 'PATCH', thread, { pin: moved })).status).toBe(403);
  expect((await write(await s.call(viewer, 'PATCH', thread, { pin: moved }), 200)).thread.pin).toEqual(moved);
  expect((await write(await s.call(owner, 'PATCH', thread, { pin }), 200)).thread.pin).toEqual(pin);
  // Отказ не тратит номер.
  const seq = (await list(owner, file)).seq;
  expect((await s.call(editor, 'PATCH', thread, { pin: moved })).status).toBe(403);
  expect((await list(owner, file)).seq).toBe(seq);
});

test('удаление первого комментария удаляет обсуждение для всех: список, надгробие, обсуждение, ответ', async () => {
  const { file } = await world();
  const a = await create(editor, file, 'Первый');
  await reply(viewer, file, a.thread.id, 'Второй');
  const keep = await create(owner, file, 'Другое');
  const known = (await list(viewer, file)).seq;
  const gone = await write(await s.call(editor, 'DELETE', `${base(file)}/${a.thread.id}/comments/${a.thread.first.id}`), 200);
  expect(gone.removed).toBe(true);
  expect(gone.seq).toBe(known + 1);
  // Последнее состояние — с текстом: клиент закрывает окно обсуждения по нему.
  expect(gone.thread.first.body).toBe('Первый');
  expect((await list(viewer, file)).threads.map((t) => t.id)).toEqual([keep.thread.id]);
  const after = await list(viewer, file, `?after=${known}`);
  expect(after).toEqual({ seq: gone.seq, threads: [], removed: [a.thread.id], more: false });
  expect((await s.call(viewer, 'GET', `${base(file)}/${a.thread.id}`)).status).toBe(404);
  expect((await s.call(viewer, 'POST', `${base(file)}/${a.thread.id}/comments`, { body: 'x' })).status).toBe(404);
  // Текст удалённого не хранится.
  expect(await rows('SELECT count(*)::int AS n FROM comment WHERE thread_id = $1', [a.thread.id])).toEqual([{ n: 0 }]);
});

test('текст: только текст, предел в кодовых точках; лишние поля, пустое и сверх предела — 400', async () => {
  const { file } = await world();
  const t = await create(owner, file, '  <img src=x onerror=alert(1)>\r\n\u202eстрока\u0000  ');
  expect(t.thread.first.body).toBe('<img src=x onerror=alert(1)>\nстрока');
  // 4000 четырёхбайтных знаков — 16 000 байт текста: помещаются в запрос.
  const emoji = '😀'.repeat(COMMENT_BODY_MAX);
  expect((await reply(owner, file, t.thread.id, emoji)).comment?.body).toBe(emoji);
  for (const body of ['', '   ', '\u202e', 'а'.repeat(COMMENT_BODY_MAX + 1), 5])
    expect((await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body })).status).toBe(400);
  expect((await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'x', html: '<b>' })).status).toBe(400);
  const many = Array.from({ length: 21 }, (_, i) => `@[u${i}]`).join(' ');
  expect((await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: many })).status).toBe(400);
  expect((await s.call(owner, 'POST', base(file), { pin: { ...pin, x: 1e10 }, body: 'x' })).status).toBe(400);
});

test('упоминания: строка и имя — только у кого доступ при отправке; у остальных токен остаётся без имени', async () => {
  const { file } = await world();
  const body = `@[${editor.id}] @[${stranger.id}] @[${linked.id}] @[nobody] @[${editor.id}]`;
  const t = await create(viewer, file, body);
  expect(t.comment?.body).toBe(body);
  expect(t.comment?.mentions).toEqual({ [editor.id]: 'Эдуард', [linked.id]: 'Лев' });
  expect(await rows('SELECT user_id FROM comment_mention ORDER BY user_id')).toEqual(
    [editor.id, linked.id].sort().map((user_id) => ({ user_id })),
  );
  // Ни имени, ни адреса чужого — нигде в ответах.
  const all = JSON.stringify([await list(viewer, file), await detail(viewer, file, t.thread.id)]);
  expect(all).not.toContain('Чужой');
  expect(all).not.toContain('c-stranger@');
  expect(all).not.toContain('@example.com');
  // Правка: новое упоминание — с проверкой доступа сейчас; заведённое при отправке остаётся, пока токен в тексте; снятое из
  // текста — уходит.
  await s.pool.query('DELETE FROM file_link_user WHERE user_id = $1', [linked.id]);
  const path = `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`;
  const e = await write(await s.call(viewer, 'PATCH', path, { body: `@[${linked.id}] @[${owner.id}] @[${stranger.id}]` }), 200);
  expect(e.comment?.mentions).toEqual({ [linked.id]: 'Лев', [owner.id]: 'Ольга' });
  // Упомянутые одним комментарием — по id: время у них одно.
  expect(e.thread.participants).toEqual([viewer.id, ...[linked.id, owner.id].sort()]);
  const e2 = await write(await s.call(viewer, 'PATCH', path, { body: `@[${owner.id}]` }), 200);
  expect(e2.comment?.mentions).toEqual({ [owner.id]: 'Ольга' });
  expect(e2.thread.participants).toEqual([viewer.id, owner.id]);
});

test('не прочитано — у каждого своё; открыл — прочитал; своё не бывает непрочитанным', async () => {
  const { file } = await world();
  const t = await create(owner, file, 'Вопрос');
  const unread = async (who: Person) => (await list(who, file)).threads[0].unread;
  expect(await unread(owner)).toBe(false);
  expect(await unread(viewer)).toBe(true);
  expect(await unread(editor)).toBe(true);
  expect((await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/read`)).status).toBe(200);
  expect(await unread(viewer)).toBe(false);
  expect(await unread(editor)).toBe(true);
  // Ответ читателя: у владельца снова не прочитано, у читателя — нет (ответил — прочитал).
  await reply(viewer, file, t.thread.id);
  expect(await unread(owner)).toBe(true);
  expect(await unread(viewer)).toBe(false);
  // Удалили чужой непрочитанный — прочитано.
  const r = await reply(editor, file, t.thread.id);
  expect(await unread(viewer)).toBe(true);
  await s.call(editor, 'DELETE', `${base(file)}/${t.thread.id}/comments/${r.comment!.id}`);
  expect(await unread(viewer)).toBe(false);
  expect((await detail(owner, file, t.thread.id)).thread.unread).toBe(true);
});

test('?after: изменения по номеру, страницы, `more` и номер страницы; неверное — 400', async () => {
  const { file } = await world();
  const made: CommentWriteDto[] = [];
  for (let i = 0; i < 5; i++) made.push(await create(owner, file, `т${i}`));
  expect(made.map((m) => m.seq)).toEqual([1, 2, 3, 4, 5]);
  const p1 = await list(viewer, file, '?limit=2');
  expect([p1.seq, p1.more, p1.threads.map((t) => t.first.body)]).toEqual([2, true, ['т0', 'т1']]);
  const p2 = await list(viewer, file, `?after=${p1.seq}&limit=2`);
  const p3 = await list(viewer, file, `?after=${p2.seq}&limit=2`);
  expect([p3.seq, p3.more, p3.threads.map((t) => t.first.body)]).toEqual([5, false, ['т4']]);
  // Изменение старого обсуждения поднимает его в конец.
  await reply(editor, file, made[0].thread.id);
  const changed = await list(viewer, file, '?after=5');
  expect([changed.seq, changed.threads.map((t) => [t.id, t.seq])]).toEqual([6, [[made[0].thread.id, 6]]]);
  // Ничего нового — голова файла и пусто.
  expect(await list(viewer, file, '?after=6')).toEqual({ seq: 6, threads: [], removed: [], more: false });
  for (const q of ['?after=-1', '?after=x', '?limit=0', '?limit=1.5', '?after=99999999999999999'])
    expect((await s.call(viewer, 'GET', `${base(file)}${q}`)).status, q).toBe(400);
  // Предел страницы урезается, а не отвергается.
  expect((await list(viewer, file, '?limit=100000')).threads).toHaveLength(5);
  // Файл без комментариев.
  const empty = (await s.newFile(owner, 'Пустой')).id;
  expect(await list(owner, empty)).toEqual({ seq: 0, threads: [], removed: [], more: false });
});

test('одновременные ответы: номера без пропусков, и ответ с номером N видит все изменения до N', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  const people = [owner, editor, viewer, linked, member];
  const all = await Promise.all(Array.from({ length: 25 }, (_, i) => reply(people[i % 5], file, t.thread.id, `о${i}`)));
  const seqs = all.map((r) => r.seq).sort((a, b) => a - b);
  expect(seqs).toEqual(Array.from({ length: 25 }, (_, i) => t.seq + 1 + i));
  // Ответ несёт число ответов на момент своего номера: снимок до блокировки дал бы меньше.
  for (const r of all) expect(r.thread.replies, `seq ${r.seq}`).toBe(r.seq - t.seq);
  expect((await list(owner, file)).threads[0].replies).toBe(25);
  // Время комментариев растёт вместе с номером: прочтение по времени не пропускает соседа.
  const d = await detail(owner, file, t.thread.id);
  const bySeq = [...all].sort((a, b) => a.seq - b.seq).map((r) => r.comment!.id);
  expect(d.comments.slice(1).map((c) => c.id)).toEqual(bySeq);
});

test('предел комментариев — 60 в минуту на человека, общий для всех его сеансов и вкладок', async () => {
  const { file } = await world();
  const t = await create(editor, file);
  const second = await s.signIn(editor.email, 'Эдуард');
  expect(second.cookie).not.toBe(editor.cookie);
  // Первый — уже создан: 59 ответов вперемешку из двух сеансов.
  for (let i = 0; i < 59; i++) {
    const res = await s.call(i % 2 ? second : editor, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: `${i}` });
    expect(res.status, `ответ ${i}`).toBe(201);
  }
  for (const who of [editor, second]) {
    const res = await s.call(who, 'POST', base(file), { pin, body: 'ещё' });
    expect(res.status).toBe(429);
    expect(Number(res.headers.get('retry-after'))).toBeGreaterThan(0);
  }
  // Другой человек не задет; правка — в своём пределе.
  expect((await s.call(viewer, 'POST', base(file), { pin, body: 'я' })).status).toBe(201);
  expect((await s.call(editor, 'PATCH', `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`, { body: 'правка' })).status).toBe(200);
  // Список — свой предел, 20 за 10 с на человека, общий для сеансов: первая загрузка дорогая, догоняние дешёвое.
  for (let i = 0; i < 20; i++) expect((await s.call(i % 2 ? second : editor, 'GET', base(file))).status).toBe(200);
  expect((await s.call(editor, 'GET', base(file))).status).toBe(429);
  expect((await s.call(editor, 'GET', `${base(file)}/${t.thread.id}`)).status).toBe(200);
});

test('предел обсуждения: ответ сверх — 409 THREAD_FULL без записи; страница — не больше 500', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  await s.pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT gen_random_uuid(), $1, $2, 'x', now() + make_interval(secs => i) FROM generate_series(1, $3) i`,
    [t.thread.id, owner.id, THREAD_COMMENTS_MAX - 2],
  );
  const last = await reply(viewer, file, t.thread.id, 'последний');
  expect(last.thread.replies).toBe(THREAD_COMMENTS_MAX - 1);
  const full = await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'лишний' });
  expect(full.status).toBe(409);
  expect(await full.json()).toEqual({ error: 'THREAD_FULL' });
  expect((await list(owner, file)).seq).toBe(last.seq);
  expect((await detail(owner, file, t.thread.id)).comments).toHaveLength(THREAD_COMMENTS_MAX);
  // Страница: 501 обсуждение — 500 и `more`.
  await s.pool.query(
    `INSERT INTO comment_thread (id, file_id, page_id, x, y, wx, wy, first_id, preview, preview_more, created_by, created_at, seq)
     SELECT gen_random_uuid(), $1, '1@1', 0, 0, 0, 0, gen_random_uuid(), 'x', false, $2, now(), 1000 + i FROM generate_series(1, 501) i`,
    [file, owner.id],
  );
  await s.pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at) SELECT first_id, id, created_by, 'x', created_at FROM comment_thread
     WHERE file_id = $1 AND seq > 1000`,
    [file],
  );
  const page = await list(owner, file, `?after=${last.seq}`);
  expect([page.threads.length, page.more]).toEqual([COMMENT_PAGE_MAX, true]);
});

test('подсказка упоминаний: люди с доступом без себя; поиск — буквы, а не шаблон; ссылку выключили — пришедшего нет', async () => {
  const { file } = await world();
  const people = async (who: Person, q = '') => {
    const res = await s.call(who, 'GET', `/api/files/${file}/mentions?q=${encodeURIComponent(q)}`);
    expect(res.status).toBe(200);
    return parseMentionList(await res.json())!.people;
  };
  const names = (p: { name: string }[]) => p.map((x) => x.name).sort();
  expect(names(await people(owner))).toEqual(['Вера', 'Лев', 'Мария', 'Эдуард']);
  expect(names(await people(viewer))).toEqual(['Лев', 'Мария', 'Ольга', 'Эдуард']);
  expect(names(await people(linked))).toEqual(['Вера', 'Мария', 'Ольга', 'Эдуард']);
  expect(names(await people(editor, 'ве'))).toEqual(['Вера']);
  // `%` и `_` — буквы.
  expect(await people(owner, '%')).toEqual([]);
  expect(await people(owner, '_')).toEqual([]);
  // Ссылку выключили — пришедшего по ней больше не предлагают.
  await s.pool.query('UPDATE share_link SET enabled = false WHERE file_id = $1', [file]);
  expect(names(await people(owner))).toEqual(['Вера', 'Мария', 'Эдуард']);
  expect((await s.call(owner, 'GET', `/api/files/${file}/mentions?q=${'a'.repeat(65)}`)).status).toBe(400);
  // Не больше 8.
  for (let i = 0; i < 9; i++) {
    const p = await s.signIn(`c-crowd-${i}@example.com`, `Толпа ${i}`);
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, p.id]);
    await opened(p, file);
  }
  expect(await people(owner, 'Толпа')).toHaveLength(8);
});

test('M2: адреса в подсказке — только тем, кто видит «Поделиться», и только у людей оттуда; адреса гостя по ссылке нет ни у кого', async () => {
  const { file } = await world();
  const people = async (who: Person, q = '') => {
    const res = await s.call(who, 'GET', `/api/files/${file}/mentions?q=${encodeURIComponent(q)}`);
    expect(res.status).toBe(200);
    return parseMentionList(await res.json())!.people;
  };
  const emails = async (who: Person) =>
    Object.fromEntries((await people(who)).map((p) => [p.name, p.email]).sort(([a], [b]) => String(a).localeCompare(String(b))));
  // Владелец, прямой редактор и участник команды видят «Поделиться»: адреса прямых и команды — да, гостя по ссылке — нет.
  expect(await emails(owner)).toEqual({ Вера: viewer.email, Лев: null, Мария: member.email, Эдуард: editor.email });
  expect(await emails(editor)).toEqual({ Вера: viewer.email, Лев: null, Мария: member.email, Ольга: owner.email });
  expect(await emails(member)).toEqual({ Вера: viewer.email, Лев: null, Ольга: owner.email, Эдуард: editor.email });
  // Прямой читатель и пришедший по ссылке — без адресов.
  for (const who of [viewer, linked]) expect(Object.values(await emails(who)).every((e) => e === null)).toBe(true);
  // Поиск по адресу — так же: читатель не находит, редактор находит, гостя по адресу не находит никто.
  expect(await people(viewer, 'c-editor')).toEqual([]);
  expect(await people(viewer, 'example.com')).toEqual([]);
  expect(await people(linked, 'c-viewer')).toEqual([]);
  expect((await people(editor, 'c-viewer')).map((p) => p.name)).toEqual(['Вера']);
  for (const who of [owner, editor, member]) expect(await people(who, 'c-linked')).toEqual([]);
});

test('M3: приглашённый, ещё не открывавший файл, в подсказке не предлагается — существующий и несуществующий адреса неотличимы', async () => {
  const { file } = await world();
  const ceo = await s.signIn('ceo@corp.example', 'Иван Секретов');
  // Владелец приглашает существующий адрес (сразу `file_access`) и несуществующий (`file_invite`).
  for (const email of ['ceo@corp.example', 'nobody@corp.example'])
    expect((await s.call(owner, 'POST', `/api/files/${file}/share/invite`, { email, role: 'viewer' })).status).toBe(200);
  expect(await rows('SELECT role FROM file_access WHERE file_id = $1 AND user_id = $2', [file, ceo.id])).toEqual([{ role: 'viewer' }]);
  const people = async (q: string) =>
    (await (await s.call(owner, 'GET', `/api/files/${file}/mentions?q=${encodeURIComponent(q)}`)).json()) as unknown;
  for (const q of ['corp.example', 'ceo', 'nobody', 'Иван', '']) {
    const got = (await people(q)) as { people: { id: string }[] };
    expect(
      got.people.some((p) => p.id === ceo.id),
      q,
    ).toBe(false);
  }
  expect(await people('corp.example')).toEqual({ people: [] });
  // Открыл файл сам — теперь его можно упомянуть.
  expect((await s.call(ceo, 'POST', `/api/files/${file}/touch`)).status).toBeLessThan(300);
  expect(await people('corp.example')).toEqual({ people: [{ id: ceo.id, name: 'Иван Секретов', email: 'ceo@corp.example' }] });
});

test('функции изменения сверяют роль сами: снятый доступ не пишет, даже минуя маршрут', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  const before = await rows('SELECT seq FROM comment_file');
  // Маршрут проверил роль, а между проверкой и записью её сняли: запись видит это сама.
  const calls: [string, unknown[]][] = [
    ['comment_create($1, $2, $3, NULL, 0, 0, 0, 0, $4, $4, false, $5, $6, 1000, 10000, 1000)', [file, stranger.id, pin.page, 'x', [], []]],
    ['comment_reply($1, $2, $3, $4, $5, $6, 1000, 500, 524288)', [file, stranger.id, t.thread.id, 'x', [], []]],
    ['comment_edit($1, $2, $3, $4, $5, $5, false, $6, 524288)', [file, stranger.id, t.thread.id, t.comment!.id, 'x', []]],
    ['comment_delete($1, $2, $3, $4)', [file, stranger.id, t.thread.id, t.comment!.id]],
    ['comment_resolve($1, $2, $3, true)', [file, stranger.id, t.thread.id]],
    ['comment_move($1, $2, $3, $4, NULL, 0, 0, 0, 0)', [file, stranger.id, t.thread.id, pin.page]],
  ];
  for (const [sql, params] of calls) {
    const e = await s.pool.query(`SELECT ${sql}`, params).catch((x: unknown) => x as { code: string });
    expect((e as { code?: string }).code, sql).toBe('CM404');
  }
  expect(await rows('SELECT seq FROM comment_file')).toEqual(before);
  // Через маршрут: доступ сняли — 404.
  await s.pool.query('DELETE FROM file_access WHERE user_id = $1', [viewer.id]);
  expect((await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'x' })).status).toBe(404);
  expect((await s.call(viewer, 'GET', base(file))).status).toBe(404);
});

test('удаление файла убирает всё о комментариях; удалённый человек — автор без имени, его упоминания и прочтения уходят', async () => {
  const { file } = await world();
  const gone = await s.signIn(`c-gone-${crypto.randomUUID()}@example.com`, 'Уходящий');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, gone.id]);
  const t = await create(gone, file, `Я и @[${owner.id}]`);
  const r = await reply(owner, file, t.thread.id, `@[${gone.id}] спасибо`);
  await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true });
  await s.call(gone, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: false });
  await s.call(gone, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true });
  await s.call(gone, 'PUT', '/api/me/prefs', { commentMail: false });
  await s.pool.query('DELETE FROM "user" WHERE id = $1', [gone.id]);
  const d = await detail(owner, file, t.thread.id);
  expect(d.thread.author).toEqual({ id: null, name: '' });
  expect(d.thread.resolvedBy).toEqual({ id: null, name: '' });
  expect(d.comments[0].author).toEqual({ id: null, name: '' });
  expect(d.comments[1].mentions).toEqual({});
  expect(d.thread.participants).toEqual([owner.id]);
  for (const [table, column] of [
    ['comment_mention', 'user_id'],
    ['comment_read', 'user_id'],
    ['user_pref', 'user_id'],
    ['mail_outbox', 'recipient_id'],
  ])
    expect(await rows(`SELECT count(*)::int AS n FROM ${table} WHERE ${column} = $1`, [gone.id]), table).toEqual([{ n: 0 }]);
  // Комментарий удалённого никто не правит.
  expect((await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`, { body: 'x' })).status).toBe(403);
  expect(r.comment).not.toBeNull();
  // Удаление файла: ни строки.
  await s.pool.query('DELETE FROM file WHERE id = $1', [file]);
  for (const table of ['comment_file', 'comment_thread', 'comment', 'comment_mention', 'comment_read', 'mail_outbox'])
    expect(await rows(`SELECT count(*)::int AS n FROM ${table}`), table).toEqual([{ n: 0 }]);
});

test('настройка писем: по умолчанию включена, своя у каждого', async () => {
  expect(await (await s.call(viewer, 'GET', '/api/me/prefs')).json()).toEqual({ commentMail: true });
  expect((await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: false })).status).toBe(200);
  expect(await (await s.call(viewer, 'GET', '/api/me/prefs')).json()).toEqual({ commentMail: false });
  expect(await (await s.call(editor, 'GET', '/api/me/prefs')).json()).toEqual({ commentMail: true });
  expect((await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: 'нет' })).status).toBe(400);
  expect((await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: true, x: 1 })).status).toBe(400);
});

const outbox = async () =>
  (
    await s.pool.query<{ recipient_id: string; comment_ids: string[]; extra: number; due_at: Date; first_at: Date; state: string }>(
      'SELECT recipient_id, comment_ids, extra, due_at, first_at, state FROM mail_outbox ORDER BY recipient_id',
    )
  ).rows;

test('очередь писем: упомянутые и участники ответа; не автор, не выключившие, не без доступа; слияние и предел', async () => {
  const { file } = await world();
  await s.call(member, 'PUT', '/api/me/prefs', { commentMail: false });
  // Создание: только упомянутые с доступом, кроме себя и выключивших.
  const t = await create(viewer, file, `@[${editor.id}] @[${viewer.id}] @[${member.id}] @[${stranger.id}]`);
  expect((await outbox()).map((o) => [o.recipient_id, o.comment_ids])).toEqual([[editor.id, [t.comment!.id]]]);
  const row = (await outbox())[0];
  // Срок — от первого и через 5 минут.
  expect(row.due_at.getTime() - row.first_at.getTime()).toBe(5 * 60 * 1000);
  // Ответ: участники обсуждения (автор первого и упомянутые), не автор ответа.
  const r = await reply(owner, file, t.thread.id, 'ответ');
  const rowsAfter = await outbox();
  const byUser = Object.fromEntries(rowsAfter.map((o) => [o.recipient_id, o.comment_ids]));
  expect(byUser).toEqual({ [editor.id]: [t.comment!.id, r.comment!.id], [viewer.id]: [r.comment!.id] });
  // Срок ждущей строки не сдвигается новыми комментариями.
  expect(rowsAfter.find((o) => o.recipient_id === editor.id)!.due_at).toEqual(row.due_at);
  // Слияние ограничено: 20 id и счётчик сверху.
  for (let i = 0; i < 25; i++) await reply(owner, file, t.thread.id, `${i}`);
  const merged = (await outbox()).find((o) => o.recipient_id === editor.id)!;
  expect([merged.comment_ids.length, merged.extra]).toEqual([20, 7]);
  // Потерявший доступ писем не получает.
  await s.pool.query('DELETE FROM file_access WHERE user_id = $1', [viewer.id]);
  await s.pool.query('DELETE FROM mail_outbox');
  await reply(owner, file, t.thread.id, 'после снятия');
  expect((await outbox()).map((o) => o.recipient_id)).toEqual([editor.id]);
});

test('очередь писем: ждущих строк у человека не больше 50 — по одной на файл', async () => {
  await world();
  const files: string[] = [];
  for (let i = 0; i < 52; i++) {
    const f = (await s.newFile(owner, `ф${i}`)).id;
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f, viewer.id]);
    await opened(viewer, f);
    files.push(f);
  }
  for (const f of files)
    await s.pool.query(`SELECT comment_create($1, $2, '1@1', NULL, 0, 0, 0, 0, $3, $3, false, $4, '{}', 1000, 10000, 1000)`, [
      f,
      owner.id,
      `@[${viewer.id}]`,
      [viewer.id],
    ]);
  expect((await outbox()).length).toBe(50);
  // В уже ждущую строку новое по-прежнему сливается.
  await s.pool.query(`SELECT comment_create($1, $2, '1@1', NULL, 0, 0, 0, 0, $3, $3, false, $4, '{}', 1000, 10000, 1000)`, [
    files[0],
    owner.id,
    `@[${viewer.id}]`,
    [viewer.id],
  ]);
  expect((await outbox()).find((o) => o.comment_ids.length === 2)).toBeDefined();
});

test('запись комментариев и настроек не сверяет роли открытых соединений', async () => {
  const { file } = await world();
  const one = spyOn(s.hub, 'recheckFile');
  const all = spyOn(s.hub, 'recheckAll');
  try {
    const t = await create(owner, file);
    await reply(viewer, file, t.thread.id);
    await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true });
    await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/read`);
    await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: false });
    expect([one.mock.calls.length, all.mock.calls.length]).toEqual([0, 0]);
    // Сверка по-прежнему идёт после записей, меняющих доступ.
    await s.call(owner, 'PATCH', `/api/files/${file}`, { name: 'Новое' });
    expect(one.mock.calls.length).toBe(1);
  } finally {
    one.mockRestore();
    all.mockRestore();
  }
});

/** Номер файла занят чужой транзакцией: изменения файла ждут её. `blocked(n)` — дождаться, что ждут n запросов. */
async function holdSeq(file: string) {
  const holder = await s.pool.connect();
  await holder.query('BEGIN');
  await holder.query('SELECT comment_next_seq($1)', [file]);
  const blocked = async (n: number) => {
    // Условие, а не время: запросы стоят в очереди блокировок базы.
    while ((await rows('SELECT count(*)::int AS n FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0'))[0].n < n)
      await Bun.sleep(5);
  };
  const release = async () => {
    await holder.query('ROLLBACK');
    holder.release();
  };
  return { blocked, release };
}

test('роль сверяется после блокировки номера: доступ сняли, пока изменение ждало соседа, — записи нет', async () => {
  const { file, other } = await world();
  const v = await create(viewer, file, 'Читатель');
  const r = await reply(viewer, file, v.thread.id, 'Ответ');
  const before = await rows('SELECT seq FROM comment_file');
  const thread = `${base(file)}/${v.thread.id}`;
  const actions: [string, string, unknown?][] = [
    ['POST', base(file), { pin, body: 'новое' }],
    ['POST', `${thread}/comments`, { body: 'ещё' }],
    ['PATCH', `${thread}/comments/${r.comment!.id}`, { body: 'правка' }],
    ['DELETE', `${thread}/comments/${r.comment!.id}`],
    ['PATCH', thread, { resolved: true }],
    ['PATCH', thread, { pin: { ...pin, x: 99 } }],
  ];
  for (const [method, path, body] of actions) {
    const seq = await holdSeq(file);
    // Маршрут проверил роль и ждёт номер файла; доступ снимают и фиксируют, пока он ждёт.
    const res = s.call(viewer, method, path, body);
    await seq.blocked(1);
    await s.pool.query('DELETE FROM file_access WHERE file_id = $1 AND user_id = $2', [file, viewer.id]);
    await seq.release();
    expect((await res).status, `${method} ${path}`).toBe(404);
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, viewer.id]);
  }
  expect(await rows('SELECT seq FROM comment_file')).toEqual(before);
  const d = await detail(viewer, file, v.thread.id);
  expect([d.thread.resolvedAt, d.thread.pin, d.comments.map((c) => c.body)]).toEqual([null, pin, ['Читатель', 'Ответ']]);
  // Файл удалили, пока изменение ждало: как несуществующий, а не сбой сервера.
  const seq = await holdSeq(file);
  const gone = s.pool.query('DELETE FROM file WHERE id = $1', [file]);
  await seq.blocked(1);
  const res = s.call(owner, 'POST', base(file), { pin, body: 'поздно' });
  await seq.blocked(2);
  await seq.release();
  await gone;
  expect((await res).status).toBe(404);
  // Наоборот: новое обсуждение уже ждёт номер, файл удаляют — сначала обсуждение, потом удаление, без взаимной блокировки
  // (обсуждение берёт строку файла раньше номера, как и удаление файла).
  await create(owner, other, 'есть');
  const busy = await holdSeq(other);
  const first = s.call(owner, 'POST', base(other), { pin, body: 'успею' });
  await busy.blocked(1);
  const removal = s.pool.query('DELETE FROM file WHERE id = $1', [other]);
  await busy.blocked(2);
  await busy.release();
  expect((await first).status).toBe(201);
  expect((await removal).rowCount).toBe(1);
});

test('удаление человека — новый номер обсуждениям, где он был: догоняющий по `?after=` видит автора без имени и участников', async () => {
  const { file, other } = await world();
  const gone = await s.signIn(`c-gone-${crypto.randomUUID()}@example.com`, 'Уходящий');
  for (const f of [file, other])
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [f, gone.id]);
  const authored = await create(gone, file, 'Моё');
  const mentioned = await create(owner, file, `@[${gone.id}] посмотри`);
  const replied = await create(owner, other, 'Там');
  await reply(gone, other, replied.thread.id, 'Ответ');
  // Только закрыл: не участник.
  const closed = await create(owner, file, 'Закрою');
  await write(await s.call(gone, 'PATCH', `${base(file)}/${closed.thread.id}`, { resolved: true }), 200);
  const untouched = await create(owner, file, 'Без него');
  const known = { [file]: (await list(owner, file)).seq, [other]: (await list(owner, other)).seq };
  await s.pool.query('DELETE FROM "user" WHERE id = $1', [gone.id]);
  const a = await list(owner, file, `?after=${known[file]}`);
  expect(a.threads.map((t) => t.id).sort()).toEqual([authored.thread.id, mentioned.thread.id, closed.thread.id].sort());
  expect(a.seq).toBe(known[file] + 3);
  const byId = new Map(a.threads.map((t) => [t.id, t]));
  expect(byId.get(authored.thread.id)!.author).toEqual({ id: null, name: '' });
  expect(byId.get(authored.thread.id)!.participants).toEqual([]);
  expect(byId.get(mentioned.thread.id)!.participants).toEqual([owner.id]);
  expect(byId.get(mentioned.thread.id)!.first.mentions).toEqual({});
  expect(byId.get(closed.thread.id)!.resolvedBy).toEqual({ id: null, name: '' });
  const b = await list(owner, other, `?after=${known[other]}`);
  expect([b.seq, b.threads.map((t) => [t.id, t.participants])]).toEqual([known[other] + 1, [[replied.thread.id, [owner.id]]]]);
  // Обсуждение без него номера не получило.
  expect((await list(owner, file)).threads.find((t) => t.id === untouched.thread.id)!.seq).toBe(untouched.seq);
});

test('удаление первого комментария владельцем файла удаляет чужое обсуждение для всех', async () => {
  const { file } = await world();
  const a = await create(viewer, file, 'Читатель');
  await reply(editor, file, a.thread.id, 'Ответ');
  const known = (await list(editor, file)).seq;
  // Редактор чужое обсуждение не удаляет, владелец — удаляет.
  expect((await s.call(editor, 'DELETE', `${base(file)}/${a.thread.id}/comments/${a.thread.first.id}`)).status).toBe(403);
  const gone = await write(await s.call(owner, 'DELETE', `${base(file)}/${a.thread.id}/comments/${a.thread.first.id}`), 200);
  expect([gone.removed, gone.seq]).toEqual([true, known + 1]);
  expect(await list(viewer, file, `?after=${known}`)).toEqual({ seq: known + 1, threads: [], removed: [a.thread.id], more: false });
  expect((await list(viewer, file)).threads).toEqual([]);
  expect((await s.call(viewer, 'GET', `${base(file)}/${a.thread.id}`)).status).toBe(404);
  expect(await rows('SELECT count(*)::int AS n FROM comment WHERE thread_id = $1', [a.thread.id])).toEqual([{ n: 0 }]);
});

test('страницы `?after=`: больше 500 надгробий и смесь с живыми — не больше 500 на страницу, номер страницы — последней строки', async () => {
  const { file } = await world();
  const known = (await create(owner, file, 'начало')).seq;
  // 600 надгробий, потом 300 живых вперемешку с 300 надгробиями.
  await s.pool.query(
    `INSERT INTO comment_thread (id, file_id, page_id, x, y, wx, wy, first_id, preview, preview_more, created_by, created_at, seq, deleted_at)
     SELECT gen_random_uuid(), $1, '1@1', 0, 0, 0, 0, gen_random_uuid(), 'x', false, $2, now(), 1000 + i,
            CASE WHEN i <= 600 OR i % 2 = 0 THEN now() END
     FROM generate_series(1, 1200) i`,
    [file, owner.id],
  );
  await s.pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at) SELECT first_id, id, created_by, 'x', created_at FROM comment_thread
     WHERE file_id = $1 AND seq > 1000 AND deleted_at IS NULL`,
    [file],
  );
  await s.pool.query('UPDATE comment_file SET seq = 2200 WHERE file_id = $1', [file]);
  const pages: CommentListDto[] = [];
  for (let after = known, more = true; more;) {
    const p = await list(viewer, file, `?after=${after}`);
    pages.push(p);
    after = p.seq;
    more = p.more;
  }
  expect(pages.map((p) => [p.threads.length, p.removed.length, p.more, p.seq])).toEqual([
    [0, 500, true, 1500],
    [200, 300, true, 2000],
    [100, 100, false, 2200],
  ]);
  // Первая загрузка — без надгробий: только живые.
  const first = await list(viewer, file);
  expect([first.threads.length, first.removed.length, first.more]).toEqual([301, 0, false]);
});

test('пределы чтения комментариев — короткие окна: список 20 за 10 с, обсуждение целиком 30 за 10 с, каждый свой', async () => {
  const { file } = await world();
  const t = await create(editor, file);
  for (let i = 0; i < 30; i++) expect((await s.call(editor, 'GET', `${base(file)}/${t.thread.id}`)).status, `обсуждение ${i}`).toBe(200);
  expect((await s.call(editor, 'GET', `${base(file)}/${t.thread.id}`)).status).toBe(429);
  // Общий `read` (1200 в минуту) пропустил бы и 31-й; другие маршруты и другие люди не задеты.
  expect((await s.call(editor, 'GET', `/api/files/${file}/mentions`)).status).toBe(200);
  for (let i = 0; i < 20; i++) expect((await s.call(editor, 'GET', base(file))).status, `список ${i}`).toBe(200);
  const res = await s.call(editor, 'GET', base(file));
  expect(res.status).toBe(429);
  expect(Number(res.headers.get('retry-after'))).toBeLessThanOrEqual(10);
  expect((await s.call(viewer, 'GET', `${base(file)}/${t.thread.id}`)).status).toBe(200);
});

test('один человек не занимает пул базы: в работе — не больше своих мест, лишние ждут без соединения, сверх очереди — 429 BUSY', async () => {
  const slots = new UserSlots({ running: 2, waiting: 10 });
  const tight = await apiStand({ slots, commentMail: false }, true);
  try {
    const { file } = await world();
    const t = await create(editor, file);
    const seq = await holdSeq(file);
    // Двенадцать ответов разом: два — в базе (ждут номер файла), десять — в очереди мест, без соединения базы.
    const replies = Array.from({ length: 12 }, (_, i) =>
      tight.call(editor, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: `${i}` }),
    );
    await seq.blocked(2);
    while (slots.count(editor.id).waiting < 10) await Bun.sleep(1);
    expect(slots.count(editor.id)).toEqual({ running: 2, waiting: 10 });
    // Очередь полна — сразу 429 `BUSY`, без базы.
    const over = await tight.call(editor, 'GET', base(file));
    expect([over.status, await over.json()]).toEqual([429, { error: 'BUSY' }]);
    // Другой человек пулом не обделён: отвечает сразу, пока ответы первого ещё ждут.
    expect((await tight.call(viewer, 'GET', base(file))).status).toBe(200);
    expect(await rows('SELECT count(*)::int AS n FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0')).toEqual([{ n: 2 }]);
    await seq.release();
    expect((await Promise.all(replies)).map((r) => r.status)).toEqual(Array(12).fill(201));
    expect(slots.count(editor.id)).toEqual({ running: 0, waiting: 0 });
  } finally {
    await tight.stop();
  }
});

test('места в работе: ушедший из очереди (срок вышел) места не занимает, освобождённое переходит следующему', async () => {
  const slots = new UserSlots({ running: 1, waiting: 2 });
  const a = slots.take('u')!;
  await a.ready;
  const b = slots.take('u')!;
  const c = slots.take('u')!;
  expect(slots.take('u')).toBeNull();
  b.release();
  expect(slots.count('u')).toEqual({ running: 1, waiting: 1 });
  a.release();
  await c.ready;
  expect(slots.count('u')).toEqual({ running: 1, waiting: 0 });
  c.release();
  c.release();
  expect(slots.count('u')).toEqual({ running: 0, waiting: 0 });
});

/** Живые обсуждения напрямую в базе: `n` штук человека `who`, без комментариев (пределы считают обсуждения). */
const seedThreads = (file: string, who: Person, n: number) =>
  s.pool.query(
    `INSERT INTO comment_thread (id, file_id, page_id, x, y, wx, wy, first_id, preview, preview_more, created_by, created_at, seq)
     SELECT gen_random_uuid(), $1, '1@1', 0, 0, 0, 0, gen_random_uuid(), 'x', false, $2, now(),
            (SELECT COALESCE(max(seq), 0) FROM comment_thread WHERE file_id = $1) + i
     FROM generate_series(1, $3) i`,
    [file, who.id, n],
  );

test('H1: обсуждений в файле — не больше 10 000, у человека в файле — не больше 1000; сверх — 409 без записи и номера', async () => {
  const { file, other } = await world();
  // Файл: 9 999 чужих, 10 000-е проходит, 10 001-е — 409 FILE_FULL, и у другого человека тоже.
  await seedThreads(file, member, COMMENT_THREADS_MAX - 1);
  await s.pool.query(`INSERT INTO comment_file (file_id, seq) VALUES ($1, $2)`, [file, COMMENT_THREADS_MAX]);
  await create(viewer, file);
  const seq = await rows('SELECT seq FROM comment_file WHERE file_id = $1', [file]);
  for (const who of [viewer, owner]) {
    const res = await s.call(who, 'POST', base(file), { pin, body: 'лишнее' });
    expect([res.status, await res.json()]).toEqual([409, { error: 'FILE_FULL' }]);
  }
  expect(await rows('SELECT seq FROM comment_file WHERE file_id = $1', [file])).toEqual(seq);
  expect(await rows('SELECT count(*)::int AS n FROM comment_thread WHERE file_id = $1', [file])).toEqual([{ n: COMMENT_THREADS_MAX }]);
  // Удалённое не считается: место освободилось — новое проходит.
  await s.pool.query(`UPDATE comment_thread SET deleted_at = now() WHERE id = (SELECT id FROM comment_thread WHERE file_id = $1 LIMIT 1)`, [
    file,
  ]);
  await create(owner, file);
  // Человек: 999 своих, 1000-е проходит, 1001-е — 409 AUTHOR_FULL; другому в том же файле — можно.
  await seedThreads(other, viewer, COMMENT_THREADS_PER_USER_MAX - 1);
  await s.pool.query(`INSERT INTO comment_file (file_id, seq) VALUES ($1, $2)`, [other, COMMENT_THREADS_PER_USER_MAX]);
  await create(viewer, other);
  const mine = await s.call(viewer, 'POST', base(other), { pin, body: 'лишнее' });
  expect([mine.status, await mine.json()]).toEqual([409, { error: 'AUTHOR_FULL' }]);
  await create(editor, other);
});

test('H1: список — начало первого текста и «есть ещё», целиком — в обсуждении; страница 500 предельных обсуждений — меньше 1 МБ', async () => {
  const { file } = await world();
  const long = 'Длинный текст. '
    .repeat(400)
    .slice(0, COMMENT_BODY_MAX - 100)
    .trim();
  const t = await create(editor, file, `@[${viewer.id}] ${long}`);
  expect(t.thread.first.more).toBe(true);
  expect(commentLength(t.thread.first.body)).toBeLessThanOrEqual(COMMENT_PREVIEW_MAX);
  expect(t.thread.first.mentions).toEqual({ [viewer.id]: 'Вера' });
  const [listed] = (await list(owner, file)).threads;
  expect(listed.first).toEqual(t.thread.first);
  expect((await detail(owner, file, t.thread.id)).comments[0].body).toBe(`@[${viewer.id}] ${long}`);
  // Правка первого меняет и начало; короткое — без «есть ещё».
  const edited = await write(
    await s.call(editor, 'PATCH', `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`, { body: 'коротко' }),
    200,
  );
  expect([edited.thread.first.body, edited.thread.first.more]).toEqual(['коротко', false]);
  // 500 обсуждений с предельными текстами разных видов (кириллица, четырёхбайтные, упоминания) — одна страница.
  const bodies = ['я'.repeat(COMMENT_BODY_MAX), '😀'.repeat(COMMENT_BODY_MAX), `@[${viewer.id}] `.repeat(20) + 'ж'.repeat(3000)];
  for (const [i, body] of bodies.entries()) {
    const p = commentPreview(body);
    await s.pool.query(
      `SELECT comment_create($1, $2, '1@1', NULL, k, 0, k, 0, $3, $4, $5, $6, '{}', 0, 10000, 1000) FROM generate_series(1, $7) k`,
      [file, [owner, editor, viewer][i].id, body, p.text, p.more, i === 2 ? [viewer.id] : [], i === 0 ? 168 : 166],
    );
  }
  const res = await s.call(owner, 'GET', `${base(file)}?after=${edited.seq}`);
  const text = await res.text();
  const page = parseCommentList(JSON.parse(text))!;
  console.log('страница 500 предельных обсуждений, байт:', text.length, new TextEncoder().encode(text).length);
  expect(page.threads).toHaveLength(COMMENT_PAGE_MAX);
  expect(page.threads.every((x) => x.first.more)).toBe(true);
  // Было 8,3 МБ (полные тексты). Сейчас ~0,63 МБ: начало текста — до 280 кодовых точек (до 1,1 КБ у четырёхбайтных), а
  // остальное обсуждение (id, метка, автор, участники) — ~0,68 КБ и без текста; только четырёхбайтные — ~0,9 МБ.
  expect(new TextEncoder().encode(text).length).toBeLessThan(1024 * 1024);
});

test('M1: текст обсуждения — не больше 512 КБ: ответ и правка сверх — 409 THREAD_FULL без записи', async () => {
  const { file } = await world();
  const t = await create(owner, file);
  const big = 'ж'.repeat(COMMENT_BODY_MAX); // 8000 байт
  await s.pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT gen_random_uuid(), $1, $2, $3, now() + make_interval(secs => i) FROM generate_series(1, $4) i`,
    [t.thread.id, viewer.id, big, Math.floor(THREAD_BYTES_MAX / 8000)],
  );
  const near = await reply(viewer, file, t.thread.id, 'ещё');
  const over = await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: big });
  expect([over.status, await over.json()]).toEqual([409, { error: 'THREAD_FULL' }]);
  const grow = await s.call(viewer, 'PATCH', `${base(file)}/${t.thread.id}/comments/${near.comment!.id}`, { body: big });
  expect([grow.status, await grow.json()]).toEqual([409, { error: 'THREAD_FULL' }]);
  expect((await list(owner, file)).seq).toBe(near.seq);
  // Правка, которая не растит текст, проходит.
  await write(await s.call(viewer, 'PATCH', `${base(file)}/${t.thread.id}/comments/${near.comment!.id}`, { body: 'ещ' }), 200);
  const d = await detail(owner, file, t.thread.id);
  expect(d.comments.reduce((n, c) => n + new TextEncoder().encode(c.body).length, 0)).toBeLessThanOrEqual(THREAD_BYTES_MAX);
});

test('M1: список и обсуждение всех людей — под общим семафором; остальные маршруты не ждут, сверх очереди — 429 BUSY', async () => {
  const heavySlots = new UserSlots({ running: 2, waiting: 3 });
  const tight = await apiStand({ heavySlots, commentMail: false }, true);
  try {
    const { file } = await world();
    const t = await create(editor, file);
    // Оба места заняты: тяжёлые чтения трёх людей ждут, не держа соединения базы.
    const held = [heavySlots.take('')!, heavySlots.take('')!];
    const waiting = [
      tight.call(viewer, 'GET', base(file)),
      tight.call(editor, 'GET', `${base(file)}/${t.thread.id}`),
      tight.call(linked, 'GET', base(file)),
    ];
    while (heavySlots.count('').waiting < 3) await Bun.sleep(1);
    const over = await tight.call(owner, 'GET', base(file));
    expect([over.status, await over.json()]).toEqual([429, { error: 'BUSY' }]);
    // Лёгкие маршруты и запись — сразу.
    expect((await tight.call(owner, 'GET', `/api/files/${file}/mentions`)).status).toBe(200);
    expect((await tight.call(owner, 'GET', '/api/me/prefs')).status).toBe(200);
    expect((await tight.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: 'пока ждут' })).status).toBe(201);
    for (const h of held) h.release();
    expect((await Promise.all(waiting)).map((r) => r.status)).toEqual([200, 200, 200]);
    expect(heavySlots.count('')).toEqual({ running: 0, waiting: 0 });
  } finally {
    await tight.stop();
  }
});

test('L2: правка, перенос и закрытие — свой предел 60 в минуту, а не общий `write`', async () => {
  const { file } = await world();
  const t = await create(viewer, file);
  const ops = [
    () => s.call(viewer, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: true }),
    () => s.call(viewer, 'PATCH', `${base(file)}/${t.thread.id}`, { pin }),
    () => s.call(viewer, 'PATCH', `${base(file)}/${t.thread.id}/comments/${t.comment!.id}`, { body: 'правка' }),
  ];
  for (let i = 0; i < 60; i++) expect((await ops[i % 3]()).status, `${i}`).toBe(200);
  for (const op of ops) expect((await op()).status).toBe(429);
  // Общий `write` не задет: прочтение проходит.
  expect((await s.call(viewer, 'POST', `${base(file)}/${t.thread.id}/read`)).status).toBe(200);
});

test('L4: невидимые знаки убираются, текст только из них — 400', async () => {
  const { file } = await world();
  for (const body of ['​‌⁠', 'ㅤ', '‍', '́́'])
    expect((await s.call(owner, 'POST', base(file), { pin, body })).status, JSON.stringify(body)).toBe(400);
  const t = await create(owner, file, 'а​б⁣в 👩‍💻' + '́'.repeat(30));
  expect(t.comment!.body).toBe('а' + 'бв 👩‍💻' + '́'.repeat(8));
});
