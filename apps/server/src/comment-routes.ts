// Комментарии (спека подсистемы 4, раздел 10): обсуждения с меткой, ответы, правка и удаление, закрытие и перенос метки,
// прочтение, подсказка упоминаний и настройка писем. Права — по действующей роли: комментировать может каждый с доступом,
// и читатель тоже (раздел 10.3). Каждый обработчик — один запрос к базе: чтение — один `SELECT`, изменение — одна функция
// базы (`010_comments.sql`), которая сама сверяет роль, поднимает номер файла и ставит письма в очередь. id из адреса и тела
// не доверяются: всякий запрос ограничен `file_id` из адреса, чужое обсуждение неотличимо от несуществующего.
//
// Объём ограничен: обсуждений в файле и у человека в нём — пределы протокола, текст обсуждения — `THREAD_BYTES_MAX`, список —
// с началом первого текста, а не целиком. Список и обсуждение — тяжёлые чтения (`heavy`): общий на процесс семафор, JSON
// отдаётся как его собрала база, без разбора и сборки заново.

import {
  COMMENT_PAGE_MAX,
  COMMENT_THREADS_MAX,
  COMMENT_THREADS_PER_USER_MAX,
  commentPreview,
  MENTION_SUGGEST_MAX,
  mentionIds,
  parseCommentBody,
  parseMentionQuery,
  parseNewThread,
  parsePrefs,
  parseThreadPatch,
  THREAD_BYTES_MAX,
  THREAD_COMMENTS_MAX,
  type CommentAction,
  type CommentDto,
  type CommentEvent,
  type MentionCandidate,
  type ThreadDto,
  type ThreadView,
} from '@ferrum/protocol';
import { apiError, ok, okJson, readJson, type ApiDeps, type Ctx, type Route } from './api';

/** Группировка писем о комментариях: первое ждёт столько, следующие в это окно сливаются в одно письмо (раздел 10.4). */
export const COMMENT_MAIL_DELAY_MS = 5 * 60 * 1000;
/** Тело запроса комментария: 4000 кодовых точек по 4 байта и метка не помещаются в общие 16 КБ. */
const COMMENT_JSON_BYTES = 32 * 1024;

/** Что функция изменения возвращает из базы. */
interface Changed {
  seq: number;
  thread: ThreadDto;
  unread: boolean;
  comment: CommentDto | null;
  removed: boolean;
  deleted: string | null;
}

/** Ошибки функций базы: отказ без записи, номер не потрачен. `23503` — файл или человек удалены, пока вызов ждал
 * блокировку номера: ушедшее — как несуществующее. */
const REFUSALS: Record<string, () => Response> = {
  CM404: () => apiError(404, 'NOT_FOUND'),
  '23503': () => apiError(404, 'NOT_FOUND'),
  CM403: () => apiError(403, 'FORBIDDEN'),
  CM409: () => apiError(409, 'THREAD_FULL'),
  CMF09: () => apiError(409, 'FILE_FULL'),
  CMU09: () => apiError(409, 'AUTHOR_FULL'),
};

function wholeParam(v: string | null, fallback: number): number | null {
  if (v === null) return fallback;
  if (!/^\d{1,16}$/.test(v)) return null;
  const n = Number(v);
  return Number.isSafeInteger(n) ? n : null;
}

/** `%`, `_` и `\` в строке поиска — буквы, а не шаблон: иначе `%` нашёл бы всех. */
const likeEscape = (s: string) => s.replace(/[\\%_]/g, (c) => `\\${c}`);

export function commentRoutes(deps: ApiDeps): Route[] {
  const { pool } = deps;
  const delay = deps.commentMailDelayMs ?? COMMENT_MAIL_DELAY_MS;

  /** Изменение: функция базы, ответ и событие соединениям файла (после фиксации — порядок по номеру держит концентратор). */
  async function change(ctx: Ctx, action: CommentAction, status: number, sql: string, params: unknown[]): Promise<Response> {
    let r: Changed;
    try {
      const { rows } = await pool.query<{ r: Changed }>(`SELECT ${sql} AS r`, params);
      r = rows[0].r;
    } catch (e) {
      const refusal = REFUSALS[(e as { code?: string }).code ?? ''];
      if (refusal) return refusal();
      throw e;
    }
    const file = ctx.params.id;
    const by = ctx.user.id;
    const event: CommentEvent = r.removed
      ? { kind: 'removed', seq: r.seq, by, thread: r.thread.id }
      : { kind: 'thread', seq: r.seq, by, action, thread: r.thread, comment: r.comment, deleted: r.deleted };
    deps.comments?.event(file, event);
    const view: ThreadView = { ...r.thread, unread: r.unread };
    return ok({ seq: r.seq, thread: view, comment: r.comment, removed: r.removed }, status);
  }

  /** Кто сейчас держит файл открытым: им письмо о комментарии не нужно, они видят его сразу. */
  const openUsers = (file: string): string[] => deps.comments?.openUsers(file) ?? [];

  const comments = (path: string) => `/api/files/:id/comments${path}`;

  return [
    {
      method: 'GET',
      path: comments(''),
      access: { kind: 'file', min: 'viewer' },
      limit: 'commentList',
      heavy: true,
      keepsAccess: true,
      handle: async (ctx) => {
        const after = wholeParam(ctx.url.searchParams.get('after'), 0);
        const limit = wholeParam(ctx.url.searchParams.get('limit'), COMMENT_PAGE_MAX);
        if (after === null || limit === null || limit === 0) return apiError(400, 'BAD_REQUEST');
        const count = Math.min(limit, COMMENT_PAGE_MAX);
        // Один снимок: голова файла и страница согласованы. Первая загрузка (`after=0`) — без надгробий.
        // Обсуждение — текстом JSON из базы: страница не разбирается и не собирается заново (`CommentListDto`).
        const { rows } = await pool.query<{ id: string | null; seq: string | null; view: string | null; head: string }>(
          `WITH h AS (SELECT COALESCE((SELECT seq FROM comment_file WHERE file_id = $1), 0) AS head)
           SELECT h.head, p.id, p.seq, p.view FROM h LEFT JOIN LATERAL (
             SELECT t.id, t.seq,
                    CASE WHEN t.deleted_at IS NULL
                      THEN (comment_thread_json(t) || jsonb_build_object('unread', comment_unread(t.id, $2)))::text END AS view
             FROM comment_thread t
             WHERE t.file_id = $1 AND t.seq > $3 AND ($3 > 0 OR t.deleted_at IS NULL)
             ORDER BY t.seq LIMIT $4) p ON true
           ORDER BY p.seq`,
          [ctx.params.id, ctx.user.id, after, count + 1],
        );
        // Пустая страница — одна строка с головой файла и без обсуждения.
        const found = rows.filter((r) => r.id !== null);
        const more = found.length > count;
        const page = found.slice(0, count);
        const seq = Number(more ? page[page.length - 1].seq : rows[0].head);
        const threads = page.flatMap((r) => (r.view ? [r.view] : []));
        const removed = page.flatMap((r) => (r.view ? [] : [r.id!]));
        return okJson(`{"seq":${seq},"threads":[${threads.join(',')}],"removed":${JSON.stringify(removed)},"more":${more}}`);
      },
    },
    {
      method: 'POST',
      path: comments(''),
      access: { kind: 'file', min: 'viewer' },
      limit: 'comment',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseNewThread(await readJson(ctx.req, COMMENT_JSON_BYTES));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { pin } = body;
        const file = ctx.params.id;
        const preview = commentPreview(body.body);
        return change(ctx, 'create', 201, 'comment_create($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)', [
          file,
          ctx.user.id,
          pin.page,
          pin.node,
          pin.x,
          pin.y,
          pin.wx,
          pin.wy,
          body.body,
          preview.text,
          preview.more,
          mentionIds(body.body),
          openUsers(file),
          delay,
          COMMENT_THREADS_MAX,
          COMMENT_THREADS_PER_USER_MAX,
        ]);
      },
    },
    {
      method: 'GET',
      path: comments('/:threadId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'commentThread',
      heavy: true,
      keepsAccess: true,
      handle: async (ctx) => {
        // Обсуждение другого файла через этот адрес — как несуществующее: условие на `file_id` из адреса. Целиком, одним
        // ответом (`ThreadDetailDto`): объём держит `THREAD_BYTES_MAX`. JSON — текстом из базы.
        const { rows } = await pool.query<{ body: string }>(
          `SELECT jsonb_build_object(
                    'thread', comment_thread_json(t) || jsonb_build_object('unread', comment_unread(t.id, $2)),
                    'comments', (SELECT COALESCE(jsonb_agg(comment_json(c) ORDER BY c.id <> t.first_id, c.created_at, c.id), '[]'::jsonb)
                                 FROM comment c WHERE c.thread_id = t.id))::text AS body
           FROM comment_thread t WHERE t.id = $3 AND t.file_id = $1 AND t.deleted_at IS NULL`,
          [ctx.params.id, ctx.user.id, ctx.params.threadId],
        );
        return rows[0] ? okJson(rows[0].body) : apiError(404, 'NOT_FOUND');
      },
    },
    {
      method: 'PATCH',
      path: comments('/:threadId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'commentEdit',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseThreadPatch(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const { id: file, threadId: thread } = ctx.params;
        if ('resolved' in body)
          return change(ctx, body.resolved ? 'resolve' : 'reopen', 200, 'comment_resolve($1, $2, $3, $4)', [
            file,
            ctx.user.id,
            thread,
            body.resolved,
          ]);
        const { pin } = body;
        return change(ctx, 'move', 200, 'comment_move($1, $2, $3, $4, $5, $6, $7, $8, $9)', [
          file,
          ctx.user.id,
          thread,
          pin.page,
          pin.node,
          pin.x,
          pin.y,
          pin.wx,
          pin.wy,
        ]);
      },
    },
    {
      method: 'POST',
      path: comments('/:threadId/comments'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'comment',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseCommentBody(await readJson(ctx.req, COMMENT_JSON_BYTES));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const file = ctx.params.id;
        return change(ctx, 'reply', 201, 'comment_reply($1, $2, $3, $4, $5, $6, $7, $8, $9)', [
          file,
          ctx.user.id,
          ctx.params.threadId,
          body.body,
          mentionIds(body.body),
          openUsers(file),
          delay,
          THREAD_COMMENTS_MAX,
          THREAD_BYTES_MAX,
        ]);
      },
    },
    {
      method: 'PATCH',
      path: comments('/:threadId/comments/:commentId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'commentEdit',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parseCommentBody(await readJson(ctx.req, COMMENT_JSON_BYTES));
        if (!body) return apiError(400, 'BAD_REQUEST');
        const preview = commentPreview(body.body);
        return change(ctx, 'edit', 200, 'comment_edit($1, $2, $3, $4, $5, $6, $7, $8, $9)', [
          ctx.params.id,
          ctx.user.id,
          ctx.params.threadId,
          ctx.params.commentId,
          body.body,
          preview.text,
          preview.more,
          mentionIds(body.body),
          THREAD_BYTES_MAX,
        ]);
      },
    },
    {
      method: 'DELETE',
      path: comments('/:threadId/comments/:commentId'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      keepsAccess: true,
      handle: (ctx) =>
        change(ctx, 'delete', 200, 'comment_delete($1, $2, $3, $4)', [
          ctx.params.id,
          ctx.user.id,
          ctx.params.threadId,
          ctx.params.commentId,
        ]),
    },
    {
      // Открыл обсуждение — прочитал **(поведение)**. Прочитано всё, что видно сейчас: время самого нового комментария, а не
      // `now()` — сосед, ещё не зафиксированный, останется непрочитанным.
      method: 'POST',
      path: comments('/:threadId/read'),
      access: { kind: 'file', min: 'viewer' },
      limit: 'write',
      keepsAccess: true,
      handle: async (ctx) => {
        const { rowCount } = await pool.query(
          `INSERT INTO comment_read (thread_id, user_id, read_at)
           SELECT t.id, $2, COALESCE((SELECT max(created_at) FROM comment WHERE thread_id = t.id), now())
           FROM comment_thread t WHERE t.id = $3 AND t.file_id = $1 AND t.deleted_at IS NULL AND file_role_rank($1, $2) >= 1
           ON CONFLICT (thread_id, user_id) DO UPDATE SET read_at = GREATEST(comment_read.read_at, EXCLUDED.read_at)`,
          [ctx.params.id, ctx.user.id, ctx.params.threadId],
        );
        return rowCount ? ok({ ok: true }) : apiError(404, 'NOT_FOUND');
      },
    },
    {
      // Кого можно упомянуть: люди с доступом к файлу (прямой, через команду, по включённой ссылке), кроме себя.
      // **(поведение)** Адрес виден и ищется только участнику файла: опубликованная ссылка адресов не раздаёт, как `GET share`.
      method: 'GET',
      path: '/api/files/:id/mentions',
      access: { kind: 'file', min: 'viewer' },
      limit: 'read',
      keepsAccess: true,
      handle: async (ctx) => {
        const q = parseMentionQuery(ctx.url.searchParams.get('q'));
        if (q === null) return apiError(400, 'BAD_REQUEST');
        const { rows } = await pool.query<MentionCandidate>(
          `WITH me AS (SELECT file_member_rank($1, $2) >= 1 AS member),
                people AS (
                  SELECT user_id FROM file_access WHERE file_id = $1
                  UNION SELECT m.user_id FROM file f JOIN project p ON p.id = f.project_id JOIN team_member m ON m.team_id = p.team_id
                        WHERE f.id = $1
                  UNION SELECT lu.user_id FROM file_link_user lu JOIN share_link l ON l.file_id = lu.file_id AND l.enabled
                        WHERE lu.file_id = $1)
           SELECT u.id, u.name, CASE WHEN me.member THEN u.email END AS email
           FROM people p JOIN "user" u ON u.id = p.user_id, me
           WHERE u.id <> $2 AND file_role_rank($1, u.id) >= 1
             AND ($3 = '' OR u.name ILIKE '%' || $4 || '%' OR (me.member AND u.email ILIKE '%' || $4 || '%'))
           ORDER BY u.name ILIKE $4 || '%' DESC, lower(u.name), u.id
           LIMIT $5`,
          [ctx.params.id, ctx.user.id, q, likeEscape(q), MENTION_SUGGEST_MAX],
        );
        return ok({ people: rows });
      },
    },
    {
      method: 'GET',
      path: '/api/me/prefs',
      access: { kind: 'user' },
      limit: 'read',
      keepsAccess: true,
      handle: async (ctx) => {
        const { rows } = await pool.query<{ comment_mail: boolean }>('SELECT comment_mail FROM user_pref WHERE user_id = $1', [
          ctx.user.id,
        ]);
        return ok({ commentMail: rows[0]?.comment_mail ?? true });
      },
    },
    {
      method: 'PUT',
      path: '/api/me/prefs',
      access: { kind: 'user' },
      limit: 'write',
      keepsAccess: true,
      handle: async (ctx) => {
        const body = parsePrefs(await readJson(ctx.req));
        if (!body) return apiError(400, 'BAD_REQUEST');
        await pool.query(
          `INSERT INTO user_pref (user_id, comment_mail) VALUES ($1, $2)
           ON CONFLICT (user_id) DO UPDATE SET comment_mail = EXCLUDED.comment_mail`,
          [ctx.user.id, body.commentMail],
        );
        return ok(body);
      },
    },
  ];
}
