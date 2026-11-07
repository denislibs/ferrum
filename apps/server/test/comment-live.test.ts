// Живые события комментариев (спека, раздел 10.4): событие `comment` по WebSocket файла — только соединениям с
// подтверждённой ролью и доступом сейчас, в порядке номеров; медленному сокету — пропуск и `gap`, без очереди. Клиент,
// применяющий события по номеру и догоняющий по REST при пропуске, приходит к тому же, что отдаёт REST.

import {
  encodeClientFrame,
  parseCommentList,
  parseCommentWrite,
  parseServerFrame,
  SYNC_PROTOCOL,
  syncPath,
  type CommentEvent,
  type CommentWriteDto,
  type PinDto,
  type ServerFrame,
  type ThreadDto,
} from '@ferrum/protocol';
import { afterAll, beforeEach, expect, test } from 'bun:test';
import { connect as tcp, type Socket } from 'node:net';
import { apiStand, type Person } from './api-stand';
import { Client } from './sync-client';

// Предел частоты комментариев проверяет `comments.test.ts`: здесь нужен поток изменений.
const s = await apiStand({ limits: { comment: { max: 100_000, windowSeconds: 60 }, write: { max: 100_000, windowSeconds: 60 } } });
afterAll(s.stop);
const owner = await s.signIn('l-owner@example.com', 'Ольга');
const editor = await s.signIn('l-editor@example.com', 'Эдуард');
const viewer = await s.signIn('l-viewer@example.com', 'Вера');

let file = '';
let other = '';
beforeEach(async () => {
  await s.reset();
  file = (await s.newFile(owner, 'Файл')).id;
  other = (await s.newFile(owner, 'Другой')).id;
  for (const f of [file, other])
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
      f,
      editor.id,
      viewer.id,
    ]);
});

const pin: PinDto = { page: '3@7910247476534078275', node: null, x: 1, y: 2, wx: 1, wy: 2 };
const base = (f: string) => `/api/files/${f}/comments`;

async function write(res: Response, status = 201): Promise<CommentWriteDto> {
  const text = await res.text();
  expect(res.status, text).toBe(status);
  return parseCommentWrite(JSON.parse(text))!;
}

/** Соединение с подтверждённой ролью. */
async function join(who: Person, f = file): Promise<Client> {
  const c = (await Client.open(s, who, f))!;
  c.hello();
  await c.wait('role');
  return c;
}

const events = (c: Client): CommentEvent[] => c.frames.flatMap((f) => (f.t === 'comment' ? [f.event] : []));
/** Обсуждение без «не прочитано» — как в событии: оно у каждого своё. */
function strip(v: CommentWriteDto['thread']): ThreadDto {
  const t: Partial<CommentWriteDto['thread']> = { ...v };
  delete t.unread;
  return t as ThreadDto;
}

/** Тишина: события, которые могли бы прийти, успели бы. */
const quiet = () => Bun.sleep(300);

test('событие — всем соединениям файла с подтверждённой ролью, и читателю, и вкладке автора; совпадает с ответом REST', async () => {
  const a = await join(owner);
  const a2 = await join(owner);
  const v = await join(viewer);
  const e = await join(editor);
  const elsewhere = await join(editor, other);
  const made = await write(await s.call(viewer, 'POST', base(file), { pin, body: 'Привет' }));
  for (const c of [a, a2, v, e]) {
    const f = await c.wait('comment');
    expect(f.event).toEqual({
      kind: 'thread',
      seq: made.seq,
      by: viewer.id,
      action: 'create',
      thread: strip(made.thread),
      comment: made.comment,
      deleted: null,
    });
  }
  const r = await write(await s.call(editor, 'POST', `${base(file)}/${made.thread.id}/comments`, { body: 'Ответ' }));
  expect((await v.wait('comment', (f) => f.event.seq === r.seq)).event).toMatchObject({ action: 'reply', comment: r.comment });
  const del = await write(await s.call(viewer, 'DELETE', `${base(file)}/${made.thread.id}/comments/${made.comment!.id}`), 200);
  expect((await e.wait('comment', (f) => f.event.seq === del.seq)).event).toEqual({
    kind: 'removed',
    seq: del.seq,
    by: viewer.id,
    thread: made.thread.id,
  });
  // Другой файл не слышит ничего.
  await quiet();
  expect(events(elsewhere)).toEqual([]);
});

test('без `hello` событий нет; после подтверждения роли — есть', async () => {
  const silent = (await Client.open(s, viewer, file))!;
  const a = await join(owner);
  await write(await s.call(owner, 'POST', base(file), { pin, body: 'раз' }));
  await a.wait('comment');
  await quiet();
  expect(events(silent)).toEqual([]);
  silent.hello();
  await silent.wait('role');
  const two = await write(await s.call(owner, 'POST', base(file), { pin, body: 'два' }));
  expect((await silent.wait('comment')).event.seq).toBe(two.seq);
});

test('снятый доступ: событие не уходит и до сверки ролей; вышедший — соединения закрыты', async () => {
  const v = await join(viewer);
  const e = await join(editor);
  // Доступ снят мимо маршрутов: сверка ролей ещё не прошла (раз в 30 с), соединение открыто.
  await s.pool.query('DELETE FROM file_access WHERE file_id = $1 AND user_id = $2', [file, viewer.id]);
  const made = await write(await s.call(owner, 'POST', base(file), { pin, body: `секрет @[${editor.id}]` }));
  await e.wait('comment', (f) => f.event.seq === made.seq);
  await quiet();
  expect(v.closed).toBeNull();
  expect(events(v)).toEqual([]);
  // Выход на этом устройстве: соединения сессии закрыты сразу, событий больше нет.
  const second = await s.signIn(editor.email, 'Эдуард');
  const tab = await join(second);
  expect((await s.call(second, 'POST', '/api/auth/sign-out', {})).status).toBe(200);
  await tab.waitClosed();
  await write(await s.call(owner, 'POST', base(file), { pin, body: 'после выхода' }));
  await quiet();
  expect(events(tab)).toEqual([]);
});

test('файл открыт — письма о комментарии нет; закрыт — есть', async () => {
  const v = await join(viewer);
  await write(await s.call(owner, 'POST', base(file), { pin, body: `@[${viewer.id}] @[${editor.id}]` }));
  const recipients = async () => (await s.pool.query<{ r: string }>('SELECT recipient_id AS r FROM mail_outbox')).rows.map((x) => x.r);
  expect(await recipients()).toEqual([editor.id]);
  v.close();
  await v.waitClosed();
  for (let i = 0; i < 50 && s.hub.openUsers(file).includes(viewer.id); i++) await Bun.sleep(10);
  await write(await s.call(owner, 'POST', base(file), { pin, body: `@[${viewer.id}]` }));
  expect((await recipients()).sort()).toEqual([editor.id, viewer.id].sort());
  // Неподтверждённое соединение файл открытым не делает.
  await s.pool.query('DELETE FROM mail_outbox');
  const silent = (await Client.open(s, editor, file))!;
  await write(await s.call(owner, 'POST', base(file), { pin, body: `@[${editor.id}]` }));
  expect(await recipients()).toEqual([editor.id]);
  silent.close();
});

/** Клиент по правилам протокола: событие с номером «известный + 1» применяется, больший номер или `gap` — догнать по REST
 * `?after=известный`, меньший — уже учтён. Итог должен совпасть с первой загрузкой. */
async function replay(who: Person, evs: CommentEvent[]) {
  let known = 0;
  const threads = new Map<string, ThreadDto>();
  let refetches = 0;
  const refetch = async () => {
    refetches++;
    for (let more = true; more;) {
      const l = parseCommentList(await (await s.call(who, 'GET', `${base(file)}?after=${known}`)).json())!;
      for (const t of l.threads) threads.set(t.id, strip(t));
      for (const id of l.removed) threads.delete(id);
      known = l.seq;
      more = l.more;
    }
  };
  for (const ev of evs) {
    if (ev.seq <= known) continue;
    if (ev.kind === 'gap' || ev.seq !== known + 1) {
      await refetch();
      continue;
    }
    if (ev.kind === 'thread') threads.set(ev.thread.id, ev.thread);
    else threads.delete(ev.thread);
    known = ev.seq;
  }
  return { known, threads, refetches };
}

test('порядок: события одного файла приходят по возрастанию номера; клиент по ним приходит к тому же, что REST', async () => {
  const c = await join(viewer);
  const t = await write(await s.call(owner, 'POST', base(file), { pin, body: 'начало' }));
  const people = [owner, editor, viewer];
  // Ответы, правки, закрытия и удаления разом, из разных рук.
  const work: Promise<unknown>[] = [];
  for (let i = 0; i < 40; i++) {
    const who = people[i % 3];
    work.push(
      (async () => {
        const r = await write(await s.call(who, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: `о${i}` }));
        if (i % 4 === 0) await s.call(who, 'PATCH', `${base(file)}/${t.thread.id}/comments/${r.comment!.id}`, { body: `правка ${i}` });
        if (i % 5 === 0) await s.call(who, 'DELETE', `${base(file)}/${t.thread.id}/comments/${r.comment!.id}`);
        if (i % 7 === 0) await s.call(owner, 'PATCH', `${base(file)}/${t.thread.id}`, { resolved: i % 2 === 0 });
      })(),
    );
  }
  for (let i = 0; i < 10; i++) work.push(s.call(people[i % 3], 'POST', base(file), { pin, body: `новое ${i}` }));
  await Promise.all(work);
  const head = parseCommentList(await (await s.call(owner, 'GET', base(file))).json())!;
  await c.wait('comment', (f) => f.event.seq === head.seq);
  const evs = events(c);
  const seqs = evs.map((e) => e.seq);
  // Строго по возрастанию: опоздавшее не шлётся после большего.
  expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
  expect(new Set(seqs).size).toBe(seqs.length);
  const got = await replay(viewer, evs);
  console.log('события комментариев: получено', evs.length, 'из', head.seq, ', догоняний по REST', got.refetches);
  expect(got.known).toBe(head.seq);
  expect(new Map([...got.threads].sort())).toEqual(new Map(head.threads.map((x) => [x.id, strip(x)] as const).sort()));
});

/** Сокет, который не читает: настоящий TCP с WebSocket поверх, кадры разбираются вручную. */
async function rawSocket(who: Person): Promise<{ raw: Socket; frames: () => ServerFrame[] }> {
  const raw = tcp(Number(new URL(s.publicUrl).port), '127.0.0.1');
  const got: Buffer[] = [];
  raw.on('data', (b: Buffer) => got.push(b));
  raw.write(
    `GET ${syncPath(file, who.id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
      `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${who.cookie}\r\n\r\n`,
  );
  // Кадр — после ответа на открытие: раньше сервер его не читает.
  await Bun.sleep(200);
  const hello = encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
  raw.write(Buffer.concat([Buffer.from([0x82, 0x80 | hello.length, 0, 0, 0, 0]), hello]));
  const frames = (): ServerFrame[] => {
    const all = Buffer.concat(got);
    let at = all.indexOf('\r\n\r\n') + 4;
    const out: ServerFrame[] = [];
    while (at + 2 <= all.length) {
      const op = all[at] & 0x0f;
      let len = all[at + 1] & 0x7f;
      let head = 2;
      if (len === 126) {
        len = all.readUInt16BE(at + 2);
        head = 4;
      } else if (len === 127) {
        len = Number(all.readBigUInt64BE(at + 2));
        head = 10;
      }
      if (at + head + len > all.length) break;
      if (op === 2) {
        const f = parseServerFrame(new Uint8Array(all.subarray(at + head, at + head + len)));
        if (!f) throw new Error('неверный кадр сервера');
        out.push(f);
      }
      at += head + len;
    }
    return out;
  };
  return { raw, frames };
}

test('медленный читатель: события пропускаются без очереди, буфер сервера не растёт; после чтения — `gap` с головой файла', async () => {
  const { raw, frames } = await rawSocket(viewer);
  for (let i = 0; i < 100 && !frames().some((f) => f.t === 'role'); i++) await Bun.sleep(10);
  expect(frames().some((f) => f.t === 'role')).toBe(true);
  raw.pause();
  const fast = await join(editor);
  // Крупные события (первый комментарий и ответ — по 4000 четырёхбайтных знаков, ~32 КБ на событие): буфер ядра ОС
  // заполняется, дальше копит сервер — если не пропускает.
  const big = '😀'.repeat(4000);
  const t = await write(await s.call(owner, 'POST', base(file), { pin, body: big }));
  const skipped0 = s.hub.commentSkipped;
  let peak = 0;
  let last = t;
  for (let i = 0; i < 400 && s.hub.commentSkipped - skipped0 < 20; i++) {
    last = await write(await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: big }));
    peak = Math.max(peak, s.hub.bufferedBytes());
  }
  for (let i = 0; i < 20; i++) {
    last = await write(await s.call(owner, 'POST', `${base(file)}/${t.thread.id}/comments`, { body: big }));
    peak = Math.max(peak, s.hub.bufferedBytes());
  }
  await fast.wait('comment', (f) => f.event.seq === last.seq);
  console.log('медленный читатель комментариев', { skipped: s.hub.commentSkipped - skipped0, peakBufferedKb: peak / 1024 });
  expect(s.hub.commentSkipped - skipped0).toBeGreaterThanOrEqual(20);
  // Буфер — до порога и одного события (64 КБ кадр), сколько бы событий ни было.
  expect(peak).toBeLessThan(64 * 1024 + 64 * 1024 + 4096);
  // Быстрый получил всё подряд.
  const fastSeqs = events(fast).map((e) => e.seq);
  expect(fastSeqs.at(-1)).toBe(last.seq);
  expect(fastSeqs).toEqual(Array.from({ length: fastSeqs.length }, (_, i) => fastSeqs[0] + i));
  raw.resume();
  let gap: CommentEvent | undefined;
  for (let i = 0; i < 300 && !gap; i++) {
    await Bun.sleep(20);
    gap = frames()
      .flatMap((f) => (f.t === 'comment' ? [f.event] : []))
      .find((e) => e.kind === 'gap');
  }
  expect(gap).toEqual({ kind: 'gap', seq: last.seq });
  raw.destroy();
}, 120_000);
