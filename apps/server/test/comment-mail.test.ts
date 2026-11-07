// Письма о комментариях (спека, раздел 10.4): группировка, кому не слать, отказы SMTP, не больше одного раза при падении и
// перезапуске, безопасность письма. Почта — поддельная, но настоящая по поведению: отказывает по заказу, зависает, считает
// попытки. Последний тест шлёт настоящее письмо через nodemailer в Mailpit стенда и читает его через API Mailpit.

import { parseCommentWrite, type CommentWriteDto, type PinDto } from '@ferrum/protocol';
import { afterAll, afterEach, beforeEach, expect, spyOn, test } from 'bun:test';
import { CommentMailWorker, type CommentMailOptions } from '../src/comment-mail';
import { smtpMailer, type Mail, type Mailer } from '../src/mail';
import { apiStand, type Person } from './api-stand';
import { Client } from './sync-client';

// Своя отправка стенда выключена: тесты гоняют свою, с поддельной почтой. Группировка — минута: «срок вышел» тест делает сам.
const s = await apiStand({ commentMail: false, commentMailDelayMs: 60_000 });
afterAll(s.stop);
const owner = await s.signIn('m-owner@example.com', 'Ольга');
const editor = await s.signIn('m-editor@example.com', 'Эдуард');
const viewer = await s.signIn('m-viewer@example.com', 'Вера');

/** Почта, которая отказывает по заказу и может зависнуть: каждое обращение — попытка. */
class FakeMail implements Mailer {
  readonly attempts: Mail[] = [];
  readonly delivered: Mail[] = [];
  readonly failures: unknown[] = [];
  /** Все отправки, и зависшие: тест дожидается их конца, а не времени. */
  readonly sending: Promise<void>[] = [];
  hang: Promise<void> | null = null;
  private readonly waiters: (() => void)[] = [];
  send(mail: Mail): Promise<void> {
    const p = this.deliver(mail);
    this.sending.push(p.catch(() => {}));
    return p;
  }
  private async deliver(mail: Mail): Promise<void> {
    this.attempts.push(mail);
    for (const w of this.waiters.splice(0)) w();
    if (this.hang) await this.hang;
    const e = this.failures.shift();
    if (e) throw e;
    this.delivered.push(mail);
  }
  /** Дождаться `n`-й попытки отправки. */
  async attempted(n: number): Promise<void> {
    while (this.attempts.length < n) await new Promise<void>((resolve) => this.waiters.push(resolve));
  }
}

const workers: CommentMailWorker[] = [];
async function worker(mail: Mailer, opts: Partial<CommentMailOptions> = {}, allowedEmails: string[] = []): Promise<CommentMailWorker> {
  const w = new CommentMailWorker(s.pool, mail, s.publicUrl, (f) => s.hub.openUsers(f), allowedEmails, {
    pollMs: 3_600_000,
    retryMs: 0,
    ...opts,
  });
  await w.start();
  workers.push(w);
  return w;
}

let file = '';
beforeEach(async () => {
  await s.reset();
  for (const key of (await s.valkey.send('KEYS', ['api:*'])) as string[]) await s.valkey.del(key);
  file = (await s.newFile(owner, 'Макет')).id;
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor'), ($1, $3, 'viewer')`, [
    file,
    editor.id,
    viewer.id,
  ]);
});
afterEach(async () => {
  for (const w of workers.splice(0)) await w.stop();
});

const pin: PinDto = { page: '3@7910247476534078275', node: null, x: 1, y: 2, wx: 1, wy: 2 };
const base = (f: string) => `/api/files/${f}/comments`;
async function write(res: Response): Promise<CommentWriteDto> {
  const text = await res.text();
  expect(res.status, text).toBeLessThan(300);
  return parseCommentWrite(JSON.parse(text))!;
}
const create = async (who: Person, body: string, f = file) => write(await s.call(who, 'POST', base(f), { pin, body }));
const reply = async (who: Person, thread: string, body: string) =>
  write(await s.call(who, 'POST', `${base(file)}/${thread}/comments`, { body }));
/** Срок группировки вышел. */
const due = () => s.pool.query(`UPDATE mail_outbox SET due_at = now() - interval '1 second' WHERE state = 'pending'`);
const states = async () =>
  (await s.pool.query<{ state: string; attempts: number }>('SELECT state, attempts FROM mail_outbox ORDER BY id')).rows.map(
    (r) => `${r.state}/${r.attempts}`,
  );

test('группировка: комментарии за окно — одно письмо со ссылками на обсуждения; до срока — ничего; второй проход — ничего', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const a = await create(owner, `@[${viewer.id}] посмотри шапку`);
  const b = await create(editor, `И подвал, @[${viewer.id}]`);
  await reply(owner, a.thread.id, 'уточнение');
  await w.tick();
  expect(mail.attempts).toHaveLength(0);
  await due();
  await w.tick();
  await w.tick();
  expect(mail.delivered.map((m) => m.to)).toEqual([viewer.email]);
  // Ответ владельца в его же обсуждении — упомянутой там Вере; редактор в нём не участвует.
  const m = mail.delivered[0];
  expect(m.subject).toBe('Новые комментарии в «Макет»: 3');
  for (const t of [a.thread.id, b.thread.id]) expect(m.text).toContain(`${s.publicUrl}/#/file/${file}?comment=${t}`);
  expect(m.text).toContain('@Вера посмотри шапку');
  expect(await states()).toEqual(['sent/1']);
});

test('не шлётся: файл открыт сейчас, открыт после первого комментария, доступ снят, письма выключены, комментарии удалены', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const cases: [string, () => Promise<unknown>][] = [
    [
      'открыт сейчас',
      async () => {
        const c = (await Client.open(s, viewer, file))!;
        c.hello();
        await c.wait('role');
        return c;
      },
    ],
    ['открыт после', () => s.call(viewer, 'POST', `/api/files/${file}/touch`)],
    ['доступ снят', () => s.pool.query('DELETE FROM file_access WHERE user_id = $1', [viewer.id])],
    ['письма выключены', () => s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: false })],
    [
      'удалено',
      async () => {
        const t = (await s.pool.query<{ id: string; first_id: string }>('SELECT id, first_id FROM comment_thread')).rows[0];
        return s.call(owner, 'DELETE', `${base(file)}/${t.id}/comments/${t.first_id}`);
      },
    ],
  ];
  for (const [name, condition] of cases) {
    await s.pool.query('DELETE FROM comment_thread');
    await s.pool.query('DELETE FROM mail_outbox');
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer') ON CONFLICT DO NOTHING`, [
      file,
      viewer.id,
    ]);
    await s.call(viewer, 'PUT', '/api/me/prefs', { commentMail: true });
    await create(owner, `@[${viewer.id}] ${name}`);
    expect(await states(), name).toEqual(['pending/0']);
    const held = await condition();
    await due();
    await w.tick();
    expect(await states(), name).toEqual(['skipped/1']);
    if (held instanceof Client) {
      held.close();
      await held.waitClosed();
    }
  }
  expect(mail.attempts).toHaveLength(0);
  expect(w.counts.skipped).toBe(cases.length);
});

test('отказ SMTP: повтор с паузой; после 5 попыток — брошено; обрыв посреди передачи и зависание — без повтора', async () => {
  const mail = new FakeMail();
  const w = await worker(mail, { sendTimeoutMs: 300 });
  await create(owner, `@[${viewer.id}] раз`);
  await due();
  // Сервер ответил кодом: письмо точно не ушло — повтор.
  mail.failures.push(Object.assign(new Error('451'), { responseCode: 451, command: 'DATA' }));
  await w.tick();
  expect(await states()).toEqual(['pending/1']);
  await due();
  await w.tick();
  expect(await states()).toEqual(['sent/2']);
  expect([mail.attempts.length, mail.delivered.length]).toEqual([2, 1]);

  // Соединение не открылось пять раз подряд — брошено.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] два`);
  for (let i = 0; i < 5; i++) {
    mail.failures.push(Object.assign(new Error('refused'), { code: 'ESOCKET', command: 'CONN' }));
    await due();
    await w.tick();
  }
  expect(await states()).toEqual(['dropped/5']);
  await due();
  await w.tick();
  expect(mail.attempts).toHaveLength(2 + 5);

  // Обрыв посреди DATA без ответа сервера: могло дойти — повтора нет.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] три`);
  await due();
  mail.failures.push(Object.assign(new Error('reset'), { code: 'ECONNECTION', command: 'DATA' }));
  await w.tick();
  await due();
  await w.tick();
  expect(await states()).toEqual(['uncertain/1']);

  // SMTP не ответил в срок: исход неизвестен, повтора нет, и поздний ответ отметку не меняет.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] четыре`);
  await due();
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  const before = mail.attempts.length;
  await w.tick();
  expect(await states()).toEqual(['uncertain/1']);
  mail.hang = null;
  release();
  // Поздний ответ SMTP пришёл (письмо дошло) — отметку он не меняет.
  await Promise.all(mail.sending);
  expect(mail.delivered.at(-1)?.text).toContain('четыре');
  await due();
  await w.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts.length - before).toBe(1);
});

test('перезапуск посреди отправки и падение между отправкой и отметкой: второй раз письмо не уходит', async () => {
  const mail = new FakeMail();
  // Первый процесс: SMTP завис, строка взята.
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  const first = await worker(mail);
  await create(owner, `@[${viewer.id}] перезапуск`);
  await due();
  const flight = first.tick();
  await mail.attempted(1);
  expect(await states()).toEqual(['sending/1']);
  // Новый процесс на той же базе: взятое прошлым — исход неизвестен.
  const second = await worker(mail);
  await second.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(second.counts.uncertain).toBe(1);
  // Прошлый процесс «очнулся» и дослал: его отметка строку не трогает, повтора нет.
  mail.hang = null;
  release();
  await flight;
  await due();
  await second.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts).toHaveLength(1);
  // Отметка прошлого процесса не прошла — и в его счётчик не попала.
  expect(first.counts.sent).toBe(0);

  // Письмо ушло, а отметка не записалась (база отказала): после перезапуска — не снова.
  await s.pool.query('DELETE FROM mail_outbox');
  await create(owner, `@[${viewer.id}] без отметки`);
  await due();
  await s.pool.query(`CREATE FUNCTION test_no_sent() RETURNS trigger LANGUAGE plpgsql AS $f$
    BEGIN IF NEW.state = 'sent' THEN RAISE EXCEPTION 'сбой теста'; END IF; RETURN NEW; END $f$`);
  await s.pool.query('CREATE TRIGGER test_no_sent BEFORE UPDATE ON mail_outbox FOR EACH ROW EXECUTE FUNCTION test_no_sent()');
  try {
    await second.tick();
  } finally {
    await s.pool.query('DROP FUNCTION test_no_sent CASCADE');
  }
  expect(mail.delivered).toHaveLength(2);
  expect(await states()).toEqual(['sending/1']);
  const third = await worker(mail);
  await due();
  await third.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts).toHaveLength(2);

  // И без перезапуска: строка в `sending` дольше срока аренды — неизвестна, не снова.
  const leased = await worker(mail, { leaseMs: 60_000 });
  await s.pool.query(`UPDATE mail_outbox SET state = 'sending', sending_at = now() - interval '1 hour', done_at = NULL`);
  await leased.tick();
  expect(await states()).toEqual(['uncertain/1']);
  expect(mail.attempts).toHaveLength(2);
});

test('повтор после отказа сливается с новой строкой того же человека и файла: одно письмо со всем', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const a = await create(owner, `@[${viewer.id}] первое`);
  await due();
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  mail.failures.push(Object.assign(new Error('452'), { responseCode: 452, command: 'RCPT TO' }));
  const flight = w.tick();
  await mail.attempted(1);
  // Пока первое отправлялось, пришло второе: новая ждущая строка.
  const b = await create(editor, `@[${viewer.id}] второе`);
  expect(await states()).toEqual(['sending/1', 'pending/0']);
  mail.hang = null;
  release();
  await flight;
  expect(await states()).toEqual(['skipped/1', 'pending/1']);
  await due();
  await w.tick();
  expect(mail.delivered).toHaveLength(1);
  for (const t of [a.thread.id, b.thread.id]) expect(mail.delivered[0].text).toContain(`?comment=${t}`);
});

test('предел писем человеку в час: сверх — строки ждут', async () => {
  const mail = new FakeMail();
  const w = await worker(mail, { perHour: 2 });
  for (let i = 0; i < 3; i++) {
    const f = (await s.newFile(owner, `Ф${i}`)).id;
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f, viewer.id]);
    await create(owner, `@[${viewer.id}]`, f);
  }
  await due();
  await w.tick();
  await w.tick();
  expect(mail.delivered).toHaveLength(2);
  expect((await states()).sort()).toEqual(['pending/0', 'sent/1', 'sent/1']);
});

test('письмо — текст как текст: HTML экранирован, тема одной строкой без чужих заголовков, упоминание без доступа — без имени', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  const eve = await s.signIn(`m-eve-${crypto.randomUUID()}@example.com`, '<b>Ева</b> "q"');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, eve.id]);
  await s.pool.query(`UPDATE file SET name = $2 WHERE id = $1`, [file, 'Макет\r\nBcc: evil@example.com\u202e\u200f\u061c\u0085']);
  const stranger = await s.signIn(`m-stranger-${crypto.randomUUID()}@example.com`, 'Чужой');
  await create(eve, `<script>alert(1)</script> & @[${viewer.id}] @[${stranger.id}]`);
  await due();
  await w.tick();
  const m = mail.delivered[0];
  expect(m.subject).toBe('<b>Ева</b> "q": комментарий в «Макет Bcc: evil@example.com»');
  expect(m.subject).not.toMatch(/[\r\n\u202e\u200f\u061c\u0085]/);
  expect(m.html).not.toContain('<script>');
  expect(m.html).not.toContain('<b>Ева');
  expect(m.html).toContain('&lt;script&gt;alert(1)&lt;/script&gt; &amp; @Вера @неизвестный');
  expect(m.html).toContain('&lt;b&gt;Ева&lt;/b&gt; &quot;q&quot;');
  expect(m.text).toContain('@неизвестный');
  expect(`${m.text}${m.html}`).not.toContain('Чужой');
});

test('настоящая почта: письмо через nodemailer приходит в Mailpit стенда, тема без чужих заголовков, HTML экранирован', async () => {
  const to = `m-real-${crypto.randomUUID()}@example.com`;
  const real = await s.signIn(to, 'Настоящий');
  await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, real.id]);
  await s.pool.query(`UPDATE file SET name = $2 WHERE id = $1`, [file, 'Макет\r\nBcc: evil@example.com']);
  const w = await worker(smtpMailer('smtp://127.0.0.1:11025', 'Ferrum <ferrum@localhost>'));
  const t = await create(owner, `<img src=x onerror=alert(1)> @[${real.id}]`);
  await due();
  await w.tick();
  expect(w.counts.sent).toBe(1);
  const api = 'http://127.0.0.1:18025/api/v1';
  let id = '';
  for (let i = 0; i < 100 && !id; i++) {
    const found = (await (await fetch(`${api}/search?query=${encodeURIComponent(`to:"${to}"`)}`)).json()) as {
      messages: { ID: string; Subject: string }[];
    };
    id = found.messages.find((x) => x.Subject.includes('комментарий'))?.ID ?? '';
    if (!id) await Bun.sleep(50);
  }
  expect(id).not.toBe('');
  const msg = (await (await fetch(`${api}/message/${id}`)).json()) as {
    Subject: string;
    To: { Address: string }[];
    Bcc: { Address: string }[] | null;
    HTML: string;
    Text: string;
  };
  expect(msg.To.map((x) => x.Address)).toEqual([to]);
  expect(msg.Bcc ?? []).toEqual([]);
  expect(msg.Subject).toBe('Ольга: комментарий в «Макет Bcc: evil@example.com»');
  expect(msg.HTML).toContain('&lt;img src=x onerror=alert(1)&gt; @Настоящий');
  expect(msg.HTML).not.toContain('<img');
  expect(msg.Text).toContain(`${s.publicUrl}/#/file/${file}?comment=${t.thread.id}`);
  const headers = (await (await fetch(`${api}/message/${id}/headers`)).json()) as Record<string, string[]>;
  expect(Object.keys(headers).map((k) => k.toLowerCase())).not.toContain('bcc');
});

test('остановка посреди прохода: взятые, но не отданные SMTP строки — обратно в очередь, без потраченной попытки', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  await create(owner, `@[${viewer.id}] первое`);
  await create(owner, `@[${editor.id}] второе`);
  await due();
  const { promise: hang, resolve: release } = Promise.withResolvers<void>();
  mail.hang = hang;
  const flight = w.tick();
  await mail.attempted(1);
  expect(await states()).toEqual(['sending/1', 'sending/1']);
  const stopping = w.stop();
  mail.hang = null;
  release();
  await stopping;
  await flight;
  // Какая из двух взята первой — порядок равных сроков не задан.
  expect((await states()).sort()).toEqual(['pending/0', 'sent/1']);
  // Следующий процесс отправляет её как обычно, а не признаёт неизвестной.
  const next = await worker(mail);
  await next.tick();
  expect(await states()).toEqual(['sent/1', 'sent/1']);
  expect([mail.attempts.length, next.counts.uncertain]).toEqual([2, 0]);
});

test('сбой базы до SMTP: строка — обратно в очередь, следующий проход её отправляет', async () => {
  const mail = new FakeMail();
  const w = await worker(mail);
  await create(owner, `@[${viewer.id}] сбой`);
  await due();
  const pool = s.pool as unknown as { query: (...a: unknown[]) => Promise<unknown> };
  const real = pool.query.bind(pool);
  const spy = spyOn(pool, 'query').mockImplementation((...a: unknown[]) =>
    typeof a[0] === 'string' && a[0].includes('AS opened_after') ? Promise.reject(new Error('сбой теста')) : real(...a),
  );
  try {
    await w.tick();
  } finally {
    spy.mockRestore();
  }
  expect(await states()).toEqual(['pending/0']);
  expect(mail.attempts).toHaveLength(0);
  await w.tick();
  expect(await states()).toEqual(['sent/1']);
  expect(w.counts.uncertain).toBe(0);
});

test('база недоступна при запуске: таймер всё равно идёт, проход сам доделывает запуск и шлёт', async () => {
  const mail = new FakeMail();
  await create(owner, `@[${viewer.id}] потом`);
  await due();
  const pool = s.pool as unknown as { query: (...a: unknown[]) => Promise<unknown> };
  const real = pool.query.bind(pool);
  let failed = false;
  const spy = spyOn(pool, 'query').mockImplementation((...a: unknown[]) => {
    if (!failed && typeof a[0] === 'string' && a[0].includes(`WHERE state = 'sending' RETURNING id`)) {
      failed = true;
      return Promise.reject(new Error('сбой теста'));
    }
    return real(...a);
  });
  const w = new CommentMailWorker(s.pool, mail, s.publicUrl, (f) => s.hub.openUsers(f), [], { pollMs: 5, retryMs: 0 });
  workers.push(w);
  try {
    await expect(w.start()).rejects.toThrow('сбой теста');
    await mail.attempted(1);
  } finally {
    spy.mockRestore();
  }
  await w.stop();
  expect(await states()).toEqual(['sent/1']);
});

test('предел писем в час считает и `uncertain`: такое письмо могло дойти', async () => {
  const mail = new FakeMail();
  const w = await worker(mail, { perHour: 2 });
  for (let i = 0; i < 3; i++) {
    const f = (await s.newFile(owner, `Ф${i}`)).id;
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [f, viewer.id]);
    await create(owner, `@[${viewer.id}]`, f);
  }
  // Одно — исход неизвестен в этот час.
  await s.pool.query(`UPDATE mail_outbox SET state = 'uncertain', done_at = now() WHERE id = (SELECT min(id) FROM mail_outbox)`);
  await due();
  await w.tick();
  await w.tick();
  expect(mail.delivered).toHaveLength(1);
  expect((await states()).sort()).toEqual(['pending/0', 'sent/1', 'uncertain/0']);
});

test('адрес не из разрешённых (`allowedEmails`) — письма нет, строка пропущена', async () => {
  const mail = new FakeMail();
  const w = await worker(mail, {}, ['allowed.example']);
  await create(owner, `@[${viewer.id}] мимо`);
  await due();
  await w.tick();
  expect(await states()).toEqual(['skipped/1']);
  expect([mail.attempts.length, w.counts.skipped]).toEqual([0, 1]);
});
