// Письма о комментариях (спека подсистемы 4, раздел 10.4): очередь `mail_outbox` в Postgres. Строку ставит в очередь та
// же транзакция, что пишет комментарий (`comment_enqueue`): одна ждущая строка на получателя и файл, срок — от первого
// комментария, за это время следующие сливаются в одно письмо. Здесь — отправка.
//
// Не больше одного раза: строка берётся на отправку (`sending`) и это фиксируется до SMTP. Удача — `sent`; известный отказ
// SMTP (сервер ответил кодом, соединение не открылось) — строка возвращается в очередь с паузой, после `MAX_ATTEMPTS` —
// `dropped`. Строка, оставшаяся в `sending` (процесс упал между отправкой и отметкой, отметка не записалась, отправка
// зависла дольше срока), не отправляется снова: `uncertain` и запись в журнал. Лишнее письмо хуже потерянного уведомления:
// комментарий всё равно виден в файле. Строка, взятая, но точно не отданная SMTP (остановка посреди прохода, сбой базы до
// отправки), возвращается в очередь (`pending`), а не в `uncertain`.
//
// Перед отправкой — снова: доступ есть, письма не выключены, файл не открыт сейчас и не открывался после первого
// комментария, комментарии ещё есть. Иначе — `skipped`.

import { splitCommentBody, type CommentDto } from '@ferrum/protocol';
import type { Pool } from 'pg';
import { emailAllowed } from './auth';
import { escapeHtml, mailFailure, type Mail, type Mailer } from './mail';

export interface CommentMailOptions {
  /** Как часто смотреть в очередь. Срок группировки (5 минут) — у строки, а не здесь. */
  pollMs: number;
  /** Строк за проход. */
  batch: number;
  /** Сколько ждать SMTP: дольше — исход неизвестен, строка `uncertain`. */
  sendTimeoutMs: number;
  /** Строка в `sending` дольше этого — исход неизвестен (`uncertain`). Больше `sendTimeoutMs`. */
  leaseMs: number;
  /** Писем одному человеку в час: сверх — строки ждут и копят комментарии дальше. */
  perHour: number;
  /** Попыток при известном отказе SMTP. */
  maxAttempts: number;
  /** Пауза перед повтором: `retryMs × номер попытки`. */
  retryMs: number;
  /** Сколько хранить отработанные строки: предел в час считается по ним. */
  keepMs: number;
}

export const COMMENT_MAIL_OPTIONS: CommentMailOptions = {
  pollMs: 15_000,
  batch: 20,
  sendTimeoutMs: 60_000,
  leaseMs: 5 * 60_000,
  perHour: 30,
  maxAttempts: 5,
  retryMs: 60_000,
  keepMs: 24 * 60 * 60_000,
};

/** Текст комментария в письме — начало, не больше этого. */
const EXCERPT_CHARS = 500;
/** Тема: без управляющих и переводов строк, не длиннее. */
const SUBJECT_CHARS = 150;

interface Claimed {
  id: string;
  recipient_id: string;
  file_id: string;
  extra: number;
  attempts: number;
}

interface Content {
  email: string;
  file_name: string;
  rank: number | null;
  wants: boolean;
  opened_after: boolean;
  items: CommentDto[] | null;
}

/** Отказ, после которого письмо точно не ушло: SMTP ответил кодом или соединение не открылось. Иначе (обрыв посреди
 * передачи, неизвестная ошибка) исход неизвестен — повтор мог бы прислать второе письмо. */
export function knownSmtpFailure(e: unknown): boolean {
  const { responseCode, command } = (typeof e === 'object' && e !== null ? e : {}) as { responseCode?: unknown; command?: unknown };
  return (typeof responseCode === 'number' && responseCode >= 400) || command === 'CONN';
}

/** Управляющие (C0, DEL, C1), разделители строк и знаки направления (тот же класс, что у текста комментария): в заголовке
 * письма их быть не должно. */
const HEADER_UNSAFE_RE = /[\p{Cc}\p{Bidi_Control}\u2028\u2029]/gu;

/** Строка заголовка: без `HEADER_UNSAFE_RE` и пробелов по краям, не длиннее предела. */
export function headerSafe(s: string, max = SUBJECT_CHARS): string {
  const clean = s.replace(HEADER_UNSAFE_RE, ' ').replace(/\s+/g, ' ').trim();
  const cps = [...clean];
  return cps.length > max ? `${cps.slice(0, max - 1).join('')}…` : clean;
}

const personName = (a: CommentDto['author']) => (a.id === null ? 'Удалённый пользователь' : a.name || 'Участник');

/** Текст комментария для письма: упоминания — именами (без доступа — «@неизвестный»), начало не длиннее предела. */
function excerpt(c: CommentDto): string {
  const text = splitCommentBody(c.body, c.mentions)
    .map((p) => ('text' in p ? p.text : `@${p.name ?? 'неизвестный'}`))
    .join('');
  const cps = [...text];
  return cps.length > EXCERPT_CHARS ? `${cps.slice(0, EXCERPT_CHARS - 1).join('')}…` : text;
}

/** Письмо о комментариях файла: каждый со ссылкой на своё обсуждение. Всё чужое в HTML экранировано, тема — одной строкой. */
export function commentMail(to: string, file: string, items: CommentDto[], extra: number, publicUrl: string, fileId: string): Mail {
  const link = (thread: string) => `${publicUrl}/#/file/${fileId}?comment=${thread}`;
  const fileName = headerSafe(file, 100);
  const total = items.length + extra;
  const subject = headerSafe(
    total === 1 ? `${personName(items[0].author)}: комментарий в «${fileName}»` : `Новые комментарии в «${fileName}»: ${total}`,
  );
  const more = extra > 0 ? `И ещё ${extra} — в файле.` : '';
  const footer = 'Письма о комментариях можно выключить в настройках Ferrum.';
  const text =
    [`Комментарии в файле «${fileName}»:`, ...items.map((c) => `${personName(c.author)}:\n${excerpt(c)}\n${link(c.thread)}`), more, footer]
      .filter(Boolean)
      .join('\n\n') + '\n';
  const html =
    `<p>Комментарии в файле «${escapeHtml(fileName)}»:</p>` +
    items
      .map(
        (c) =>
          `<p><b>${escapeHtml(personName(c.author))}</b><br>${escapeHtml(excerpt(c)).replace(/\n/g, '<br>')}<br>` +
          `<a href="${escapeHtml(link(c.thread))}">Открыть обсуждение</a></p>`,
      )
      .join('') +
    (more ? `<p>${escapeHtml(more)}</p>` : '') +
    `<p>${escapeHtml(footer)}</p>`;
  return { to, subject, text, html };
}

export class CommentMailWorker {
  private readonly opts: CommentMailOptions;
  private timer: Timer | null = null;
  private running: Promise<void> | null = null;
  private stopped = false;
  /** Строки прошлого процесса в `sending` уже признаны `uncertain`: до этого проход ничего не берёт. */
  private recovering: Promise<void> | null = null;
  /** Взяты, но SMTP не отданы (остановка, сбой базы до отправки): вернуть в очередь при первой возможности. */
  private readonly untried = new Set<string>();
  /** Счётчики для тестов и пробы. */
  readonly counts = { sent: 0, skipped: 0, retried: 0, dropped: 0, uncertain: 0 };

  constructor(
    private readonly pool: Pool,
    private readonly mailer: Mailer,
    private readonly publicUrl: string,
    /** Кто держит файл открытым сейчас (соединения с подтверждённой ролью). */
    private readonly openUsers: (file: string) => string[],
    private readonly allowedEmails: string[] = [],
    opts: Partial<CommentMailOptions> = {},
  ) {
    this.opts = { ...COMMENT_MAIL_OPTIONS, ...opts };
  }

  /** Запуск: таймер — сразу, и при недоступной базе (проход сам повторит то, что не вышло здесь); строки, взятые прошлым
   * процессом и не отмеченные, — исход неизвестен, снова не шлются. */
  async start(): Promise<void> {
    this.timer = setInterval(() => void this.tick().catch(report), this.opts.pollMs);
    this.timer.unref?.();
    await this.recover();
  }

  /** Один раз за процесс, до первого взятия: иначе своё взятое признали бы чужим. */
  private recover(): Promise<void> {
    this.recovering ??= (async () => {
      const { rows } = await this.pool.query<{ id: string }>(
        `UPDATE mail_outbox SET state = 'uncertain', done_at = now() WHERE state = 'sending' RETURNING id`,
      );
      this.noteUncertain(rows.length, 'остались от прошлого запуска');
    })().catch((e: unknown) => {
      this.recovering = null;
      throw e;
    });
    return this.recovering;
  }

  /** Один проход; идущий проход не запускается второй раз. */
  tick(): Promise<void> {
    if (this.stopped) return Promise.resolve();
    this.running ??= this.round().finally(() => {
      this.running = null;
    });
    return this.running;
  }

  async stop(): Promise<void> {
    this.stopped = true;
    if (this.timer) clearInterval(this.timer);
    await this.running?.catch(() => {});
    // Проход вернул своё сам; это — на случай, если и он не смог.
    await this.returnUntried();
  }

  /** Взятые и не отданные SMTP — обратно в очередь, без потраченной попытки. Не вышло (база недоступна) — останутся до
   * следующего раза. */
  private async returnUntried(): Promise<void> {
    for (const id of this.untried) {
      try {
        await this.release(id, 0, true);
        this.untried.delete(id);
      } catch (e) {
        report(e);
        return;
      }
    }
  }

  private noteUncertain(n: number, why: string): void {
    if (n === 0) return;
    this.counts.uncertain += n;
    console.error(`Письма о комментариях: исход ${n} неизвестен (${why}), повтора не будет`);
  }

  private async round(): Promise<void> {
    const { opts, pool } = this;
    await this.recover();
    // Не отданные прошлым проходом — раньше срока аренды: иначе их признали бы `uncertain`.
    await this.returnUntried();
    const expired = await pool.query(
      `UPDATE mail_outbox SET state = 'uncertain', done_at = now()
       WHERE state = 'sending' AND sending_at < now() - make_interval(secs => $1 / 1000.0)`,
      [opts.leaseMs],
    );
    this.noteUncertain(expired.rowCount ?? 0, 'отправка дольше срока');
    await pool.query(`DELETE FROM mail_outbox WHERE done_at < now() - make_interval(secs => $1 / 1000.0)`, [opts.keepMs]);
    // Взять — и зафиксировать — до SMTP. Человеку, которому за час ушло (или уходит, или, может быть, ушло — `uncertain`)
    // `perHour` писем, строка ждёт дальше и копит комментарии; в одном проходе ему — не больше остатка.
    const { rows } = await pool.query<Claimed>(
      `WITH recent AS (
         SELECT q.recipient_id AS r, (SELECT count(*) FROM mail_outbox x
                WHERE x.recipient_id = q.recipient_id AND x.state IN ('sent', 'sending', 'uncertain')
                  AND COALESCE(x.done_at, x.sending_at) > now() - interval '1 hour') AS n
         FROM (SELECT DISTINCT recipient_id FROM mail_outbox WHERE state = 'pending' AND due_at <= now()) q),
       cand AS (
         SELECT q.id, q.recipient_id, q.due_at FROM mail_outbox q JOIN recent ON recent.r = q.recipient_id
         WHERE q.state = 'pending' AND q.due_at <= now() AND recent.n < $2
         ORDER BY q.due_at LIMIT $1 * 4 FOR UPDATE OF q SKIP LOCKED),
       ranked AS (
         SELECT c.id, c.due_at, row_number() OVER (PARTITION BY c.recipient_id ORDER BY c.due_at) + recent.n AS k
         FROM cand c JOIN recent ON recent.r = c.recipient_id)
       UPDATE mail_outbox o SET state = 'sending', sending_at = now(), attempts = o.attempts + 1
       WHERE o.id IN (SELECT id FROM ranked WHERE k <= $2 ORDER BY due_at LIMIT $1)
       RETURNING o.id, o.recipient_id, o.file_id, o.extra, o.attempts`,
      [opts.batch, opts.perHour],
    );
    for (const [i, row] of rows.entries()) {
      if (this.stopped) {
        // Остановка: остальные взятые SMTP не отдавались — обратно в очередь.
        for (const r of rows.slice(i)) this.untried.add(r.id);
        break;
      }
      await this.deliver(row).catch(report);
    }
    await this.returnUntried();
  }

  private async deliver(row: Claimed): Promise<void> {
    let c: Content | undefined;
    try {
      c = await this.content(row);
      // Файл или человек удалены — строка ушла вместе с ними.
      if (!c) return;
      const skip =
        (c.rank ?? 0) < 1 ||
        !c.wants ||
        c.opened_after ||
        !c.items?.length ||
        !emailAllowed(c.email, this.allowedEmails) ||
        this.openUsers(row.file_id).includes(row.recipient_id);
      if (skip) {
        this.counts.skipped += await this.mark(row.id, 'skipped');
        return;
      }
    } catch (e) {
      // До SMTP: письмо точно не ушло — в очередь, а не в `uncertain`.
      report(e);
      this.untried.add(row.id);
      return;
    }
    const mail = commentMail(c.email, c.file_name, c.items!, row.extra, this.publicUrl, row.file_id);
    let timer: Timer | undefined;
    const outcome = await Promise.race([
      this.mailer.send(mail).then(
        () => 'sent' as const,
        (e: unknown) => e,
      ),
      new Promise<'timeout'>((resolve) => {
        timer = setTimeout(resolve, this.opts.sendTimeoutMs, 'timeout');
      }),
    ]).finally(() => clearTimeout(timer));
    if (outcome === 'sent') {
      this.counts.sent += await this.mark(row.id, 'sent');
      return;
    }
    if (outcome === 'timeout' || !knownSmtpFailure(outcome)) {
      if (outcome !== 'timeout') console.error('Письмо о комментариях: сбой отправки,', mailFailure(outcome));
      this.noteUncertain(await this.mark(row.id, 'uncertain'), outcome === 'timeout' ? 'SMTP не ответил в срок' : 'обрыв посреди отправки');
      return;
    }
    console.error('Письмо о комментариях не принято:', mailFailure(outcome));
    if (row.attempts >= this.opts.maxAttempts) {
      const n = await this.mark(row.id, 'dropped');
      this.counts.dropped += n;
      if (n > 0) console.error(`Письмо о комментариях брошено после ${row.attempts} попыток`);
      return;
    }
    await this.release(row.id, this.opts.retryMs * row.attempts);
    this.counts.retried++;
  }

  /** Что нужно письму и проверке перед ним — одним запросом. */
  private async content(row: Claimed): Promise<Content | undefined> {
    const { rows } = await this.pool.query<Content>(
      `SELECT u.email, f.name AS file_name, file_role_rank(o.file_id, o.recipient_id) AS rank,
              COALESCE((SELECT comment_mail FROM user_pref WHERE user_id = o.recipient_id), true) AS wants,
              EXISTS (SELECT 1 FROM file_recent r WHERE r.user_id = o.recipient_id AND r.file_id = o.file_id AND r.opened_at > o.first_at)
                AS opened_after,
              (SELECT jsonb_agg(comment_json(c) ORDER BY c.created_at, c.id)
               FROM comment c JOIN comment_thread t ON t.id = c.thread_id
               WHERE c.id = ANY (o.comment_ids) AND t.file_id = o.file_id AND t.deleted_at IS NULL) AS items
       FROM mail_outbox o JOIN "user" u ON u.id = o.recipient_id JOIN file f ON f.id = o.file_id
       WHERE o.id = $1`,
      [row.id],
    );
    return rows[0];
  }

  /** Отметка только своей строки в `sending`: строку, признанную `uncertain` (срок, новый процесс), она не трогает. Сколько
   * строк отмечено (0 или 1): счётчики считают только свои отметки. */
  private async mark(id: string, state: 'sent' | 'skipped' | 'dropped' | 'uncertain'): Promise<number> {
    const { rowCount } = await this.pool.query(`UPDATE mail_outbox SET state = $2, done_at = now() WHERE id = $1 AND state = 'sending'`, [
      id,
      state,
    ]);
    return rowCount ?? 0;
  }

  /** Вернуть в очередь с паузой. Пока строка отправлялась, могла появиться новая ждущая на того же человека и файл: тогда
   * эта сливается в неё (старое — вперёд), а сама уходит. Одновременная вставка — повтор. `untried` — SMTP строку не видел:
   * попытка не тратится. */
  private async release(id: string, delayMs: number, untried = false): Promise<void> {
    for (let attempt = 0; ; attempt++) {
      try {
        await this.pool.query(
          `WITH me AS (SELECT * FROM mail_outbox WHERE id = $1 AND state = 'sending'),
                merged AS (
                  UPDATE mail_outbox p SET
                    comment_ids = (me.comment_ids || p.comment_ids)[1:20],
                    extra = p.extra + me.extra + GREATEST(0, cardinality(me.comment_ids) + cardinality(p.comment_ids) - 20),
                    first_at = LEAST(p.first_at, me.first_at),
                    due_at = LEAST(p.due_at, now() + make_interval(secs => $2 / 1000.0)),
                    attempts = GREATEST(p.attempts, me.attempts - $3)
                  FROM me WHERE p.recipient_id = me.recipient_id AND p.file_id = me.file_id AND p.state = 'pending'
                  RETURNING p.id)
           UPDATE mail_outbox o SET
             state = CASE WHEN EXISTS (SELECT 1 FROM merged) THEN 'skipped' ELSE 'pending' END,
             done_at = CASE WHEN EXISTS (SELECT 1 FROM merged) THEN now() END,
             due_at = now() + make_interval(secs => $2 / 1000.0),
             sending_at = NULL,
             attempts = o.attempts - $3
           FROM me WHERE o.id = me.id`,
          [id, delayMs, untried ? 1 : 0],
        );
        return;
      } catch (e) {
        if ((e as { code?: string }).code !== '23505' || attempt >= 2) throw e;
      }
    }
  }
}

function report(e: unknown): void {
  console.error('Письма о комментариях:', e instanceof Error ? e.message : e);
}
