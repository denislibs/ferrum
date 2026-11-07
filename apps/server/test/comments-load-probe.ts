// Нагрузка комментариев (спека подсистемы 4, разделы 10 и 13.1). Печатает итоги в JSON последней строкой.
//   big                — файл с 1000 обсуждений по 20 комментариев: задержка списка (все страницы), обсуждения, нового
//                        обсуждения и ответа по REST; планы запросов (индексы).
//   fanout SECS [hostile] — 50 соединений одного файла (5 человек по 10 вкладок) в дочернем процессе, раз в 100 мс новый
//                        комментарий по REST: задержка от ответа REST до прихода события на каждое соединение (часы одного
//                        процесса). `hostile` — ещё процесс враждебного человека: 5 сеансов, 40 потоков создают комментарии и
//                        читают список из 1000 обсуждений без остановки, и его сокет, который не читает. Процессор, память и
//                        буферы — сервера (клиенты в других процессах).

import { encodeClientFrame, parseServerFrame, SYNC_PROTOCOL, syncPath, type PinDto } from '@ferrum/protocol';
import { connect } from 'node:net';
import type { Pool } from 'pg';
import { apiStand, type Person } from './api-stand';

const [mode, ...args] = process.argv.slice(2);
const pin: PinDto = { page: '3@7910247476534078275', node: null, x: 1, y: 2, wx: 1, wy: 2 };
const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? s[Math.min(s.length - 1, Math.ceil((s.length * p) / 100) - 1)] : -1;
};
const round = (v: number) => Math.round(v * 100) / 100;

if (mode === 'clients') {
  // Дочерний процесс: 50 соединений и автор комментариев.
  const [secs, origin, file, ...cookies] = args;
  const url = origin.replace('http', 'ws');
  const users = JSON.parse(cookies.pop()!) as string[];
  const sockets: WebSocket[] = [];
  const received = new Map<number, number[]>();
  let ready = 0;
  for (let i = 0; i < 50; i++) {
    const k = i % cookies.length;
    const ws = new WebSocket(url + syncPath(file, users[k]), { headers: { cookie: cookies[k], origin } } as never);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => ws.send(encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 }));
    ws.onmessage = (e) => {
      const f = parseServerFrame(new Uint8Array(e.data as ArrayBuffer));
      if (f?.t === 'role' && ++ready === 50) console.log('ready');
      if (f?.t !== 'comment') return;
      const at = performance.now();
      let list = received.get(f.event.seq);
      if (!list) received.set(f.event.seq, (list = []));
      list.push(at);
    };
    sockets.push(ws);
  }
  while (ready < 50) await Bun.sleep(5);
  const fromResponse: number[] = [];
  const fromRequest: number[] = [];
  const rest: number[] = [];
  const sent: { seq: number; t0: number; t1: number }[] = [];
  const start = performance.now();
  let failed = 0;
  while (performance.now() - start < Number(secs) * 1000) {
    const t0 = performance.now();
    const res = await fetch(`${origin}/api/files/${file}/comments`, {
      method: 'POST',
      headers: { cookie: cookies[0], origin, 'content-type': 'application/json' },
      body: JSON.stringify({ pin, body: `замер ${t0}` }),
    });
    const t1 = performance.now();
    if (res.status !== 201) {
      failed++;
      await res.text();
    } else sent.push({ seq: ((await res.json()) as { seq: number }).seq, t0, t1 });
    rest.push(t1 - t0);
    const wait = 100 - (performance.now() - t0);
    if (wait > 0) await Bun.sleep(wait);
  }
  await Bun.sleep(1000);
  let delivered = 0;
  for (const s of sent) {
    for (const at of received.get(s.seq) ?? []) {
      delivered++;
      fromResponse.push(at - s.t1);
      fromRequest.push(at - s.t0);
    }
  }
  console.log(
    JSON.stringify({
      comments: sent.length,
      failed,
      deliveries: delivered,
      expected: sent.length * 50,
      restP50: round(pct(rest, 50)),
      restP95: round(pct(rest, 95)),
      fromResponseP50: round(pct(fromResponse, 50)),
      fromResponseP95: round(pct(fromResponse, 95)),
      fromResponseMax: round(pct(fromResponse, 100)),
      fromRequestP50: round(pct(fromRequest, 50)),
      fromRequestP95: round(pct(fromRequest, 95)),
    }),
  );
  setTimeout(() => process.exit(0), 10_000);
  await new Promise(() => {});
}

if (mode === 'hostile') {
  // Дочерний процесс враждебного: 40 потоков создают комментарии и читают большой список до сигнала.
  const [origin, file, thread, ...cookies] = args;
  const counts: Record<string, number> = {};
  let stop = false;
  process.on('SIGTERM', () => (stop = true));
  const note = (k: string) => (counts[k] = (counts[k] ?? 0) + 1);
  const big = '😀'.repeat(4000);
  const loops = Array.from({ length: 40 }, async (_, i) => {
    const cookie = cookies[i % cookies.length];
    const headers = { cookie, origin, 'content-type': 'application/json' };
    while (!stop) {
      const kind = i % 4;
      const res =
        kind === 0
          ? await fetch(`${origin}/api/files/${file}/comments`, { method: 'POST', headers, body: JSON.stringify({ pin, body: big }) })
          : kind === 1
            ? await fetch(`${origin}/api/files/${file}/comments/${thread}/comments`, {
                method: 'POST',
                headers,
                body: JSON.stringify({ body: big }),
              })
            : kind === 2
              ? await fetch(`${origin}/api/files/${file}/comments?limit=500`, { headers })
              : await fetch(`${origin}/api/files/${file}/comments/${thread}`, { headers });
      note(`${['create', 'reply', 'list', 'thread'][kind]}:${res.status}`);
      await res.arrayBuffer();
    }
  });
  console.log('started');
  await Promise.all(loops);
  console.log(JSON.stringify(counts));
  process.exit(0);
}

/** 1000 обсуждений по 20 комментариев, у каждого пятого — упоминание; прочтения у половины. */
async function seed(pool: Pool, file: string, people: Person[], threads = 1000, per = 20): Promise<void> {
  const ids = people.map((p) => p.id);
  await pool.query(
    `INSERT INTO comment_thread (id, file_id, page_id, node_id, x, y, wx, wy, first_id, created_by, created_at, seq)
     SELECT gen_random_uuid(), $1, '3@7910247476534078275', (i % 50) || '@7910247476534078275', i, i, i, i, gen_random_uuid(),
            ($2::text[])[1 + i % 5], now() - make_interval(secs => 1000000 - i * 100), i
     FROM generate_series(1, $3) i`,
    [file, ids, threads],
  );
  await pool.query(
    `INSERT INTO comment (id, thread_id, author_id, body, created_at)
     SELECT CASE WHEN k = 0 THEN t.first_id ELSE gen_random_uuid() END, t.id, ($2::text[])[1 + (k + t.seq::int) % 5],
            repeat('Текст комментария. ', 10) || CASE WHEN k % 5 = 0 THEN '@[' || ($2::text[])[1 + k % 5] || ']' ELSE '' END,
            t.created_at + make_interval(secs => k)
     FROM comment_thread t, generate_series(0, $3 - 1) k WHERE t.file_id = $1`,
    [file, ids, per],
  );
  await pool.query(
    `INSERT INTO comment_mention (comment_id, user_id)
     SELECT c.id, substring(c.body FROM '@\\[([A-Za-z0-9_-]+)\\]') FROM comment c JOIN comment_thread t ON t.id = c.thread_id
     WHERE t.file_id = $1 AND c.body LIKE '%@[%'`,
    [file],
  );
  await pool.query(
    `INSERT INTO comment_read (thread_id, user_id, read_at) SELECT id, $2, now() FROM comment_thread WHERE file_id = $1 AND seq % 2 = 0`,
    [file, ids[0]],
  );
  await pool.query('SELECT comment_thread_refresh(id) FROM comment_thread WHERE file_id = $1', [file]);
  await pool.query('INSERT INTO comment_file (file_id, seq) VALUES ($1, $2)', [file, threads]);
  await pool.query('ANALYZE comment_thread, comment, comment_mention, comment_read, comment_file');
}

async function timed(n: number, work: () => Promise<void>): Promise<{ p50: number; p95: number; max: number }> {
  const xs: number[] = [];
  for (let i = 0; i < n; i++) {
    const t0 = performance.now();
    await work();
    xs.push(performance.now() - t0);
  }
  return { p50: round(pct(xs, 50)), p95: round(pct(xs, 95)), max: round(pct(xs, 100)) };
}

const unlimited = { max: 1_000_000, windowSeconds: 60 };

if (mode === 'big') {
  const s = await apiStand({
    commentMail: false,
    limits: { comment: unlimited, read: unlimited, write: unlimited, commentThread: unlimited },
  });
  const people: Person[] = [];
  for (let i = 0; i < 5; i++) people.push(await s.signIn(`load-${i}-${Date.now()}@example.com`, `Человек ${i}`));
  const file = (await s.newFile(people[0])).id;
  for (const p of people.slice(1))
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'editor')`, [file, p.id]);
  await seed(s.pool, file, people);
  const me = people[0];
  const get = async (path: string) => {
    const res = await s.call(me, 'GET', path);
    if (res.status !== 200) throw new Error(`${path}: ${res.status}`);
    return res.json() as Promise<{ seq: number; more: boolean; threads: { id: string }[] }>;
  };
  let bytes = 0;
  const listAll = await timed(20, async () => {
    let after = 0;
    for (let more = true; more;) {
      const res = await s.call(me, 'GET', `/api/files/${file}/comments?after=${after}`);
      const text = await res.text();
      bytes = text.length;
      const l = JSON.parse(text) as { seq: number; more: boolean };
      after = l.seq;
      more = l.more;
    }
  });
  const { rows: threadRows } = await s.pool.query<{ id: string }>('SELECT id FROM comment_thread WHERE file_id = $1 ORDER BY seq', [file]);
  const thread = threadRows[500].id;
  const listAfter = await timed(50, async () => void (await get(`/api/files/${file}/comments?after=990`)));
  const detail = await timed(50, async () => void (await get(`/api/files/${file}/comments/${thread}`)));
  const create = await timed(50, async () => {
    const res = await s.call(me, 'POST', `/api/files/${file}/comments`, { pin, body: `@[${people[1].id}] новое` });
    if (res.status !== 201) throw new Error(`create: ${res.status}`);
    await res.text();
  });
  let k = 0;
  const reply = await timed(50, async () => {
    const t = threadRows[(k++ * 37) % 1000].id;
    const res = await s.call(people[k % 5], 'POST', `/api/files/${file}/comments/${t}/comments`, { body: `@[${people[0].id}] ответ` });
    if (res.status !== 201) throw new Error(`reply: ${res.status}`);
    await res.text();
  });
  const mentions = await timed(50, async () => void (await get(`/api/files/${file}/mentions?q=чел`)));
  // Планы: запросы, из которых собраны функции вида, — с настоящими параметрами.
  const plan = async (sql: string, params: unknown[]) =>
    (await s.pool.query<{ 'QUERY PLAN': string }>(`EXPLAIN (ANALYZE, COSTS OFF) ${sql}`, params)).rows
      .map((r) => r['QUERY PLAN'])
      .join('\n');
  const plans = {
    list: await plan(
      `SELECT t.id FROM comment_thread t WHERE t.file_id = $1 AND t.seq > $2 AND ($2 > 0 OR t.deleted_at IS NULL) ORDER BY t.seq LIMIT 501`,
      [file, 0],
    ),
    listAfter: await plan(
      `SELECT t.id FROM comment_thread t WHERE t.file_id = $1 AND t.seq > $2 AND ($2 > 0 OR t.deleted_at IS NULL) ORDER BY t.seq LIMIT 501`,
      [file, 990],
    ),
    threadComments: await plan(`SELECT count(*), max(created_at) FROM comment WHERE thread_id = $1`, [thread]),
    unread: await plan(
      `SELECT EXISTS (SELECT 1 FROM comment c WHERE c.thread_id = $1 AND c.author_id IS DISTINCT FROM $2
         AND c.created_at > COALESCE((SELECT read_at FROM comment_read r WHERE r.thread_id = $1 AND r.user_id = $2), '-infinity'))`,
      [thread, me.id],
    ),
    mentions: await plan(`SELECT u.id FROM comment_mention m JOIN "user" u ON u.id = m.user_id WHERE m.comment_id = $1`, [
      (await s.pool.query<{ first_id: string }>('SELECT first_id FROM comment_thread WHERE id = $1', [thread])).rows[0].first_id,
    ]),
  };
  const indexes = Object.fromEntries(
    Object.entries(plans).map(([k, v]) => [
      k,
      [...v.matchAll(/(?:Index Scan|Index Only Scan|Bitmap Index Scan)(?: Backward)? (?:using|on) (\w+)/g)].map((m) => m[1]),
    ]),
  );
  const seqScans = Object.fromEntries(Object.entries(plans).map(([k, v]) => [k, /Seq Scan on (comment|comment_thread)\b/.test(v)]));
  console.error(Object.values(plans).join('\n---\n'));
  console.log(JSON.stringify({ listAllPages: listAll, listBytes: bytes, listAfter, detail, create, reply, mentions, indexes, seqScans }));
  await s.stop();
  process.exit(0);
}

if (mode === 'fanout') {
  const [secs, hostile = ''] = args;
  const s = await apiStand({
    commentMail: false,
    // 50 соединений — вкладки пяти человек; предел частоты автора замера снят, у враждебного — настоящий.
    syncLimits: { connectionsPerUser: 100, userOpensBurst: 1000 },
  });
  const people: Person[] = [];
  for (let i = 0; i < 5; i++) people.push(await s.signIn(`fan-${i}-${Date.now()}@example.com`, `Человек ${i}`));
  const file = (await s.newFile(people[0])).id;
  for (const p of people.slice(1))
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, p.id]);
  await s.valkey.set(`api:comment:${people[0].id}`, '-100000');
  await seed(s.pool, file, people);
  const thread = (await s.pool.query<{ id: string }>('SELECT id FROM comment_thread WHERE file_id = $1 ORDER BY seq LIMIT 1', [file]))
    .rows[0].id;
  let evil: Person[] = [];
  let raw: ReturnType<typeof connect> | null = null;
  if (hostile) {
    // Пять сеансов одного человека — как вкладки и устройства: предел частоты у них общий.
    const email = `evil-${Date.now()}@example.com`;
    for (let i = 0; i < 5; i++) evil.push(await s.signIn(email, 'Враждебный'));
    await s.pool.query(`INSERT INTO file_access (file_id, user_id, role) VALUES ($1, $2, 'viewer')`, [file, evil[0].id]);
    raw = connect(Number(new URL(s.publicUrl).port), '127.0.0.1');
    raw.write(
      `GET ${syncPath(file, evil[0].id)} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
        `Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${s.publicUrl}\r\nCookie: ${evil[0].cookie}\r\n\r\n`,
    );
    await Bun.sleep(200);
    const hello = encodeClientFrame({ t: 'hello', protocol: SYNC_PROTOCOL, schema: 1 });
    raw.write(Buffer.concat([Buffer.from([0x82, 0x80 | hello.length, 0, 0, 0, 0]), hello]));
    await Bun.sleep(200);
    raw.pause();
  } else evil = [];
  Bun.gc(true);
  const rss0 = process.memoryUsage().rss;
  const heap0 = process.memoryUsage().heapUsed;
  const child = Bun.spawn(
    ['bun', import.meta.path, 'clients', secs, s.publicUrl, file, ...people.map((p) => p.cookie), JSON.stringify(people.map((p) => p.id))],
    { stdout: 'pipe', stderr: 'inherit' },
  );
  const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
  let text = '';
  const next = async (): Promise<string> => {
    for (;;) {
      const at = text.indexOf('\n');
      if (at >= 0) {
        const line = text.slice(0, at);
        text = text.slice(at + 1);
        return line;
      }
      const r = await reader.read();
      if (r.done) throw new Error('клиенты упали');
      text += new TextDecoder().decode(r.value);
    }
  };
  if ((await next()) !== 'ready') throw new Error('клиенты не готовы');
  let flood: Bun.Subprocess<'ignore', 'pipe', 'inherit'> | null = null;
  if (hostile) {
    flood = Bun.spawn(['bun', import.meta.path, 'hostile', s.publicUrl, file, thread, ...evil.map((p) => p.cookie)], {
      stdout: 'pipe',
      stderr: 'inherit',
    });
  }
  const cpu0 = process.cpuUsage();
  const t0 = performance.now();
  let peakBuffered = 0;
  let peakRss = 0;
  // Задержка цикла событий сервера: таймер на 10 мс.
  const lags: number[] = [];
  let tick = performance.now();
  const probe = setInterval(() => {
    const now = performance.now();
    lags.push(now - tick - 10);
    tick = now;
    peakBuffered = Math.max(peakBuffered, s.hub.bufferedBytes());
    peakRss = Math.max(peakRss, process.memoryUsage().rss);
  }, 10);
  const report = JSON.parse(await next()) as Record<string, number>;
  clearInterval(probe);
  const cpu = process.cpuUsage(cpu0);
  const wall = performance.now() - t0;
  let floodCounts: Record<string, number> = {};
  if (flood) {
    flood.kill('SIGTERM');
    const out = await new Response(flood.stdout).text();
    floodCounts = JSON.parse(out.trim().split('\n').at(-1) ?? '{}') as Record<string, number>;
  }
  Bun.gc(true);
  const accepted = (await s.pool.query<{ n: number }>('SELECT count(*)::int AS n FROM comment WHERE author_id = $1', [evil[0]?.id ?? '']))
    .rows[0].n;
  console.log(
    JSON.stringify({
      sockets: 50,
      hostile: !!hostile,
      ...report,
      serverCpuPercent: round(((cpu.user + cpu.system) / 1000 / wall) * 100),
      loopLagP95: round(pct(lags, 95)),
      loopLagMax: round(pct(lags, 100)),
      rssPeakGrowthMb: round((peakRss - rss0) / 1e6),
      heapGrowthMb: round((process.memoryUsage().heapUsed - heap0) / 1e6),
      peakBufferedKb: round(peakBuffered / 1024),
      commentSkipped: s.hub.commentSkipped,
      hostileAcceptedComments: accepted,
      hostile429: Object.entries(floodCounts)
        .filter(([k]) => k.endsWith(':429'))
        .reduce((n, [, v]) => n + v, 0),
      floodCounts,
    }),
  );
  child.kill();
  raw?.destroy();
  await s.stop();
  process.exit(0);
}
