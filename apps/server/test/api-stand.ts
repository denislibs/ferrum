// Стенд тестов API: настоящий сервер на свободном порту, настоящие Postgres и Valkey, настоящий
// вход по письму. Люди заводятся один раз на файл тестов, данные файлов очищаются между тестами.

import { containerHeader, SECTION_PAGE_REFS } from '@ferrum/protocol';
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Pool } from 'pg';
import { startServer, type AppDeps } from '../src/app';
import { createAuth } from '../src/auth';
import { FsBlobStore } from '../src/blob-store';
import type { Mail } from '../src/mail';
import { migrate } from '../src/migrate';
import { connectValkey } from '../src/valkey';
import { resetDatabase, resetValkey, TEST_SECRET, TEST_VALKEY_URL, testPool } from './helpers';

export interface Person {
  id: string;
  email: string;
  cookie: string;
}

function section(kind: number, id: string, data: Uint8Array): Uint8Array {
  const idBytes = new TextEncoder().encode(id);
  const out = new Uint8Array(5 + idBytes.length + 40 + data.length);
  const v = new DataView(out.buffer);
  out[0] = kind;
  v.setUint32(1, idBytes.length, true);
  out.set(idBytes, 5);
  v.setBigUint64(5 + idBytes.length, BigInt(data.length), true);
  out.set(data, 5 + idBytes.length + 40);
  return out;
}

/** Контейнер `.ferrum` нужного вида: корень и, если есть, ссылки страницы на картинки. */
export function container(refs: string[] = [], fill = 16): Uint8Array<ArrayBuffer> {
  const parts = [section(1, '', new Uint8Array(fill).fill(7))];
  if (refs.length > 0) parts.push(section(SECTION_PAGE_REFS, '1@1', new TextEncoder().encode(refs.join('\n'))));
  const head = containerHeader(parts.length);
  const out = new Uint8Array(head.length + parts.reduce((n, s) => n + s.length, 0));
  out.set(head);
  let at = head.length;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

export const PNG: Uint8Array<ArrayBuffer> = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

/** `keep` — второй стенд с другими настройками на той же базе: люди первого остаются. */
export async function apiStand(over: Partial<AppDeps> = {}, keep = false) {
  const pool: Pool = testPool();
  const valkey = await connectValkey(TEST_VALKEY_URL);
  if (!keep) {
    await resetDatabase(pool);
    await resetValkey(valkey);
    await migrate(pool, join(import.meta.dir, '../migrations'));
  }
  const free = Bun.serve({ port: 0, fetch: () => new Response() });
  const port = free.port!;
  free.stop(true);
  const publicUrl = `http://localhost:${port}`;
  const mails: Mail[] = [];
  const mailer = { send: async (m: Mail) => void mails.push(m) };
  const auth = createAuth({ publicUrl, secret: TEST_SECRET, pool, valkey, mailer, allowedEmails: [] });
  const dataDir = mkdtempSync(join(tmpdir(), 'ferrum-files-'));
  const blobs = new FsBlobStore(dataDir);
  const server = startServer(
    {
      pool,
      valkey,
      webDist: '/nonexistent',
      auth,
      publicUrl,
      trustedProxies: [],
      blobs,
      mailer,
      secret: TEST_SECRET,
      ...over,
      ...(over.compact ? { compact: { ...over.compact, init: { ...over.compact.init, dataDir } } } : {}),
    },
    port,
  );

  const call = (who: Person | null, method: string, path: string, body?: unknown, headers: Record<string, string> = {}) =>
    fetch(`${publicUrl}${path}`, {
      method,
      headers: {
        origin: publicUrl,
        ...(who ? { cookie: who.cookie } : {}),
        ...(body !== undefined && !(body instanceof Uint8Array) ? { 'content-type': 'application/json' } : {}),
        ...headers,
      },
      body: body === undefined ? undefined : body instanceof Uint8Array ? (body as Uint8Array<ArrayBuffer>) : JSON.stringify(body),
      redirect: 'manual',
    });

  /** Письмо уходит после ответа: его ждут опросом. */
  const waitMail = async (to: string, seen = 0): Promise<Mail> => {
    for (let i = 0; i < 250; i++) {
      const found = mails.filter((m) => m.to === to);
      if (found.length > seen) return found[found.length - 1];
      await Bun.sleep(20);
    }
    throw new Error(`нет письма на ${to}`);
  };

  /** Настоящий вход: письмо, подтверждение, имя. */
  const signIn = async (email: string, name = 'Имя'): Promise<Person> => {
    const seen = mails.filter((m) => m.to === email).length;
    const sent = await call(null, 'POST', '/api/auth/sign-in/magic-link', { email });
    if (sent.status !== 200) throw new Error(`вход: ${sent.status}`);
    const mail = await waitMail(email, seen);
    const token = new URLSearchParams(mail.text.split('#/auth/confirm?')[1].split('\n')[0]).get('token')!;
    const res = await call(null, 'POST', '/api/auth/confirm', { token });
    const cookie = res.headers
      .getSetCookie()
      .map((c) => c.split(';')[0])
      .join('; ');
    const who: Person = { id: '', email, cookie };
    const named = await call(who, 'POST', '/api/auth/name', { name });
    const { user } = (await named.json()) as { user: { id: string } };
    who.id = user.id;
    return who;
  };

  // Строка файла — первой, как у сервера (журнал и сжатие блокируют файл, потом документы): очистка посреди фонового
  // сжатия прошлого теста дождётся его, а не сцепится с ним.
  const reset = () =>
    pool.query(
      'TRUNCATE file, doc_state, doc_update, doc_peer, file_access, file_invite, share_link, file_link_user, file_recent, file_image, image, blob_trash, blob_pending, team, team_member, team_invite, project, comment_file, comment_thread, comment, comment_mention, comment_read, mail_outbox, user_pref CASCADE',
    );

  const stop = async () => {
    server.compactor?.stop();
    await server.hub.stop();
    server.stop(true);
    valkey.close();
    await pool.end();
    // Байты тестов: десятки прогонов иначе заполнят диск (проба памяти пишет 200 МБ).
    rmSync(dataDir, { recursive: true, force: true });
  };

  /** Новый файл и запись содержимого. */
  const newFile = async (who: Person, name = 'Файл', projectId?: string) => {
    const res = await call(who, 'POST', '/api/files', { name, projectId });
    if (res.status !== 201) throw new Error(`создание файла: ${res.status}`);
    return (await res.json()) as { id: string; version: number };
  };
  const writeContent = (who: Person, id: string, base: number, bytes: Uint8Array = container(), extra: Record<string, string> = {}) =>
    call(who, 'PUT', `/api/files/${id}/content`, bytes, {
      'if-match': `"${base}"`,
      'x-ferrum-write': crypto.randomUUID(),
      ...extra,
    });

  /** Загрузка, застывшая посреди тела: первые байты ушли, остальные — после `release`. */
  const gatedUpload = (who: Person, path: string, bytes: Uint8Array, headers: Record<string, string> = {}) => {
    const { promise: gate, resolve: release } = Promise.withResolvers<void>();
    const body = new ReadableStream<Uint8Array>({
      async start(c) {
        c.enqueue(bytes.subarray(0, 8));
        await gate;
        c.enqueue(bytes.subarray(8));
        c.close();
      },
    });
    const done = fetch(`${publicUrl}${path}`, { method: 'PUT', headers: { origin: publicUrl, cookie: who.cookie, ...headers }, body });
    return { done, release };
  };

  /** Загрузки дошли до хранилища: столько временных файлов уже открыто. */
  const waitTmp = async (count: number): Promise<void> => {
    const seen = () =>
      readdirSync(dataDir, { recursive: true, withFileTypes: true }).filter((e) => e.isFile() && e.name.endsWith('.tmp')).length;
    for (let i = 0; i < 500 && seen() < count; i++) await Bun.sleep(10);
    if (seen() < count) throw new Error(`нет ${count} временных файлов, есть ${seen()}`);
  };

  /** Сбой базы внутри `work`: запросы `op` к таблице `table` падают. */
  const failing = async <T>(table: string, op: string, work: () => Promise<T>): Promise<T> => {
    await pool.query(`CREATE FUNCTION test_failure() RETURNS trigger LANGUAGE plpgsql AS $f$ BEGIN RAISE EXCEPTION 'сбой теста'; END $f$`);
    await pool.query(`CREATE TRIGGER test_failure BEFORE ${op} ON ${table} FOR EACH ROW EXECUTE FUNCTION test_failure()`);
    try {
      return await work();
    } finally {
      await pool.query('DROP FUNCTION test_failure CASCADE');
    }
  };

  /** Намерения загрузок старше срока очистки. */
  const expireUploads = () => pool.query(`UPDATE blob_pending SET started_at = now() - interval '25 hours'`);

  return {
    pool,
    valkey,
    auth,
    publicUrl,
    mails,
    blobs,
    dataDir,
    server,
    hub: server.hub,
    compactor: server.compactor,
    call,
    signIn,
    waitMail,
    reset,
    stop,
    newFile,
    writeContent,
    gatedUpload,
    waitTmp,
    failing,
    expireUploads,
  };
}
export type ApiStand = Awaited<ReturnType<typeof apiStand>>;
