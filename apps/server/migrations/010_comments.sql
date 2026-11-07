-- Комментарии (спека подсистемы 4, раздел 10): обсуждения с меткой на холсте, комментарии, упоминания, прочтения, настройка
-- писем и очередь писем. Не в Loro: не шаг отмены, не документ, не экспорт.
--
-- Порядок и согласованность. Каждое изменение комментариев файла поднимает счётчик `comment_file.seq` первым же действием и
-- держит блокировку его строки до фиксации: изменения одного файла идут по одному, номер растёт в порядке фиксации, а у
-- обсуждения хранится номер его последнего изменения. Поэтому `?after=N` не пропускает изменений, а событие с номером N
-- несёт состояние со всеми изменениями до N. Изменения — функции ниже: в одном операторе с CTE всё читается снимком начала
-- оператора, то есть до ожидания блокировки, и ответ на ответ мог бы не видеть одновременного соседа с меньшим номером. В
-- функции каждый оператор после блокировки видит всё зафиксированное до неё. Для сервера это по-прежнему один запрос.
-- Роль функция сверяет тоже после блокировки: доступ, снятый, пока вызов ждал соседа, запись не пропускает.
-- Отказ внутри функции (нет обсуждения, нет прав, предел) — исключение с кодом `CM404`/`CM403`/`CM409`/`CMF09`/`CMU09`: номер
-- не тратится.

CREATE FUNCTION epoch_ms(t timestamptz) RETURNS bigint
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$ SELECT floor(extract(epoch FROM t) * 1000)::bigint $$;

-- Счётчик изменений комментариев файла: не в строке `file` — её держат журнал и сжатие.
CREATE TABLE comment_file (
  file_id uuid PRIMARY KEY REFERENCES file ON DELETE CASCADE,
  seq bigint NOT NULL
);

-- Обсуждение удаляется мягко (`deleted_at`): `?after=` отдаёт его надгробием. Комментарии удалённого — удаляются.
CREATE TABLE comment_thread (
  id uuid PRIMARY KEY,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  page_id text NOT NULL,
  node_id text,
  x double precision NOT NULL,
  y double precision NOT NULL,
  wx double precision NOT NULL,
  wy double precision NOT NULL,
  -- Первый комментарий: его удаление удаляет обсуждение.
  first_id uuid NOT NULL,
  -- Начало его текста для списка и события (`commentPreview` протокола, считает сервер при записи) и «текст длиннее»: список
  -- не несёт текстов целиком.
  preview text NOT NULL,
  preview_more boolean NOT NULL,
  created_by text REFERENCES "user" ON DELETE SET NULL,
  created_at timestamptz NOT NULL,
  resolved_at timestamptz,
  resolved_by text REFERENCES "user" ON DELETE SET NULL,
  seq bigint NOT NULL,
  deleted_at timestamptz,
  -- Сводка для списка (`comment_thread_refresh`, под блокировкой изменения): иначе каждая строка списка собирала бы её из
  -- комментариев, и список 1000 обсуждений стоил бы в разы дороже.
  replies integer NOT NULL DEFAULT 0,
  -- Байт текста всех комментариев: предел обсуждения (`THREAD_BYTES_MAX`).
  bytes integer NOT NULL DEFAULT 0,
  last_at timestamptz NOT NULL DEFAULT now(),
  participants text[] NOT NULL DEFAULT '{}'
);
-- Список `?after=` по номеру; живые — для первой загрузки без надгробий.
CREATE UNIQUE INDEX comment_thread_seq_idx ON comment_thread (file_id, seq);
CREATE INDEX comment_thread_live_idx ON comment_thread (file_id, seq) WHERE deleted_at IS NULL;
-- Предел обсуждений человека в файле (`comment_create`).
CREATE INDEX comment_thread_author_live_idx ON comment_thread (file_id, created_by) WHERE deleted_at IS NULL;
-- Удаление человека обнуляет ссылки: без индексов — полный просмотр таблиц на каждое удаление.
CREATE INDEX comment_thread_created_by_idx ON comment_thread (created_by);
CREATE INDEX comment_thread_resolved_by_idx ON comment_thread (resolved_by);
-- Удалённый человек уходит из участников (`comment_user_gone`).
CREATE INDEX comment_thread_participants_idx ON comment_thread USING gin (participants);

-- Комментарий удаляется сразу, без надгробия: обсуждение приходит событием и `?after=` целиком.
CREATE TABLE comment (
  id uuid PRIMARY KEY,
  thread_id uuid NOT NULL REFERENCES comment_thread ON DELETE CASCADE,
  author_id text REFERENCES "user" ON DELETE SET NULL,
  -- Нормальный текст (`normalizeCommentBody`), до 4000 кодовых точек.
  body text NOT NULL,
  created_at timestamptz NOT NULL,
  edited_at timestamptz
);
CREATE INDEX comment_thread_idx ON comment (thread_id, created_at, id);
CREATE INDEX comment_author_idx ON comment (author_id);

-- Упоминание заводится только для того, у кого был доступ к файлу при отправке.
CREATE TABLE comment_mention (
  comment_id uuid NOT NULL REFERENCES comment ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  PRIMARY KEY (comment_id, user_id)
);
CREATE INDEX comment_mention_user_idx ON comment_mention (user_id);

-- Прочтение: обсуждение не прочитано, пока есть чужой комментарий новее `read_at`.
CREATE TABLE comment_read (
  thread_id uuid NOT NULL REFERENCES comment_thread ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  read_at timestamptz NOT NULL,
  PRIMARY KEY (thread_id, user_id)
);
CREATE INDEX comment_read_user_idx ON comment_read (user_id);

CREATE TABLE user_pref (
  user_id text PRIMARY KEY REFERENCES "user" ON DELETE CASCADE,
  comment_mail boolean NOT NULL DEFAULT true
);

-- Очередь писем о комментариях (раздел 10.4): письмо не теряется при перезапуске. Одна ждущая строка на получателя и файл:
-- новые комментарии сливаются в неё (не больше 20 id, остальное — счётчиком `extra`), срок — от первого (группировка).
-- `sending` — взято на отправку и зафиксировано до SMTP: строка, найденная в нём после сбоя, не отправляется снова
-- (`uncertain`) — письмо не больше одного раза.
CREATE TABLE mail_outbox (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  recipient_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  comment_ids uuid[] NOT NULL,
  extra integer NOT NULL DEFAULT 0,
  first_at timestamptz NOT NULL,
  due_at timestamptz NOT NULL,
  state text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'sending', 'sent', 'skipped', 'dropped', 'uncertain')),
  attempts integer NOT NULL DEFAULT 0,
  sending_at timestamptz,
  done_at timestamptz
);
CREATE UNIQUE INDEX mail_outbox_pending_idx ON mail_outbox (recipient_id, file_id) WHERE state = 'pending';
CREATE INDEX mail_outbox_due_idx ON mail_outbox (due_at) WHERE state = 'pending';
CREATE INDEX mail_outbox_sending_idx ON mail_outbox (sending_at) WHERE state = 'sending';
-- Предел писем человеку в час и очистка старых строк.
CREATE INDEX mail_outbox_sent_idx ON mail_outbox (recipient_id, done_at) WHERE state = 'sent';
CREATE INDEX mail_outbox_done_idx ON mail_outbox (done_at) WHERE done_at IS NOT NULL;
CREATE INDEX mail_outbox_file_idx ON mail_outbox (file_id);

-- --- Виды для ответов: JSON прямо из базы, вид — `CommentDto`/`ThreadDto` пакета протокола. ---

CREATE FUNCTION comment_author(u text) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object('id', u, 'name', COALESCE((SELECT name FROM "user" WHERE id = u), ''))
$$;

CREATE FUNCTION comment_json(c comment) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'id', c.id, 'thread', c.thread_id, 'author', comment_author(c.author_id), 'body', c.body,
    'createdAt', epoch_ms(c.created_at), 'editedAt', epoch_ms(c.edited_at),
    'mentions', COALESCE((SELECT jsonb_object_agg(u.id, u.name) FROM comment_mention m JOIN "user" u ON u.id = m.user_id
                          WHERE m.comment_id = c.id), '{}'::jsonb))
$$;

-- Сводка обсуждения заново: число ответов, байт текста, время последнего комментария и участники — авторы и упомянутые, по первому
-- появлению (автор комментария раньше упомянутых в нём, упомянутые одним комментарием — по id), не больше 50
-- (`THREAD_PARTICIPANTS_MAX`). Зовут функции изменения после своих записей.
CREATE FUNCTION comment_thread_refresh(p_thread uuid) RETURNS void
LANGUAGE sql AS $$
  UPDATE comment_thread t SET
    replies = GREATEST(a.n - 1, 0),
    bytes = a.bytes,
    last_at = COALESCE(a.last, t.created_at),
    participants = COALESCE((
      SELECT array_agg(p.u ORDER BY p.n) FROM (
        SELECT x.u, row_number() OVER (ORDER BY min(x.at), min(x.k), x.u COLLATE "C") AS n FROM (
          SELECT c.author_id AS u, c.created_at AS at, 0 AS k FROM comment c WHERE c.thread_id = p_thread AND c.author_id IS NOT NULL
          UNION ALL
          SELECT m.user_id, c.created_at, 1 FROM comment c JOIN comment_mention m ON m.comment_id = c.id WHERE c.thread_id = p_thread
        ) x GROUP BY x.u ORDER BY n LIMIT 50
      ) p), '{}')
  FROM (SELECT count(*) AS n, COALESCE(sum(octet_length(body)), 0) AS bytes, max(created_at) AS last
        FROM comment WHERE thread_id = p_thread) a
  WHERE t.id = p_thread;
$$;

-- Удалённый человек: его комментарии без автора, упоминания ушли (внешние ключи — раньше этого триггера) — сводки
-- обсуждений, где он участвовал, заново.
CREATE FUNCTION comment_user_gone() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM comment_thread_refresh(id) FROM comment_thread WHERE participants @> ARRAY[OLD.id];
  RETURN NULL;
END
$$;
CREATE TRIGGER comment_user_gone AFTER DELETE ON "user" FOR EACH ROW EXECUTE FUNCTION comment_user_gone();

-- Удаление человека меняет обсуждения, где он автор, закрывший, участник, писал или упомянут (имя пропадает): каждое
-- получает новый номер своего файла под блокировкой счётчика, как всякое изменение, — клиенты, догоняющие по `?after=`,
-- видят нового автора и участников. До удаления: после внешних ключей `created_by` и `resolved_by` уже пусты.
CREATE FUNCTION comment_user_leaving() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  r record;
BEGIN
  FOR r IN
    SELECT t.id, t.file_id FROM comment_thread t
    WHERE t.deleted_at IS NULL AND (
      t.created_by = OLD.id OR t.resolved_by = OLD.id OR t.participants @> ARRAY[OLD.id]
      OR t.id IN (SELECT c.thread_id FROM comment c WHERE c.author_id = OLD.id)
      OR t.id IN (SELECT c.thread_id FROM comment_mention m JOIN comment c ON c.id = m.comment_id WHERE m.user_id = OLD.id))
    -- Файлы — по порядку: два удаления людей разом не ждут друг друга крест-накрест.
    ORDER BY t.file_id, t.seq
  LOOP
    UPDATE comment_thread SET seq = comment_next_seq(r.file_id) WHERE id = r.id;
  END LOOP;
  RETURN OLD;
END
$$;
CREATE TRIGGER comment_user_leaving BEFORE DELETE ON "user" FOR EACH ROW EXECUTE FUNCTION comment_user_leaving();

CREATE FUNCTION comment_thread_json(t comment_thread) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'id', t.id,
    'pin', jsonb_build_object('page', t.page_id, 'node', t.node_id, 'x', t.x, 'y', t.y, 'wx', t.wx, 'wy', t.wy),
    'author', comment_author(t.created_by),
    'createdAt', epoch_ms(t.created_at),
    'resolvedAt', epoch_ms(t.resolved_at),
    'resolvedBy', CASE WHEN t.resolved_at IS NULL THEN NULL ELSE comment_author(t.resolved_by) END,
    'seq', t.seq,
    -- Первый — началом текста (`CommentHeadDto`), с именами только тех упомянутых, кто в этом начале.
    'first', (SELECT jsonb_build_object(
                'id', c.id, 'author', comment_author(c.author_id), 'body', t.preview, 'more', t.preview_more,
                'createdAt', epoch_ms(c.created_at), 'editedAt', epoch_ms(c.edited_at),
                'mentions', COALESCE((SELECT jsonb_object_agg(u.id, u.name) FROM comment_mention m JOIN "user" u ON u.id = m.user_id
                                      WHERE m.comment_id = c.id AND strpos(t.preview, '@[' || u.id || ']') > 0), '{}'::jsonb))
              FROM comment c WHERE c.id = t.first_id),
    'replies', t.replies,
    'lastAt', epoch_ms(t.last_at),
    'participants', to_jsonb(t.participants))
$$;

-- Не прочитано: есть чужой комментарий новее прочтения. Комментарий удалённого человека — чужой.
CREATE FUNCTION comment_unread(p_thread uuid, p_user text) RETURNS boolean
LANGUAGE sql STABLE AS $$
  SELECT EXISTS (
    SELECT 1 FROM comment c
    WHERE c.thread_id = p_thread AND c.author_id IS DISTINCT FROM p_user
      AND c.created_at > COALESCE((SELECT read_at FROM comment_read r WHERE r.thread_id = p_thread AND r.user_id = p_user), '-infinity'))
$$;

-- Ответ изменения: обсуждение, «не прочитано» для того, кто менял, и изменённый комментарий.
CREATE FUNCTION comment_result(p_thread uuid, p_user text, p_comment uuid, p_removed boolean, p_deleted uuid) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'seq', t.seq, 'thread', comment_thread_json(t), 'unread', comment_unread(t.id, p_user),
    'comment', (SELECT comment_json(c) FROM comment c WHERE c.id = p_comment),
    'removed', p_removed, 'deleted', p_deleted)
  FROM comment_thread t WHERE t.id = p_thread
$$;

-- --- Изменения ---

-- Следующий номер файла; блокирует строку счётчика до конца транзакции.
CREATE FUNCTION comment_next_seq(p_file uuid) RETURNS bigint
LANGUAGE sql AS $$
  INSERT INTO comment_file AS f (file_id, seq) VALUES (p_file, 1)
  ON CONFLICT (file_id) DO UPDATE SET seq = f.seq + 1
  RETURNING seq
$$;

-- Роль — здесь же, в транзакции изменения и после `comment_next_seq`: снимок оператора берётся после ожидания
-- блокировки, и снятие роли между проверкой маршрута и записью (и пока вызов ждал соседа) не пропускает запись.
CREATE FUNCTION comment_rank(p_file uuid, p_user text) RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE
  v integer := COALESCE(file_role_rank(p_file, p_user), 0);
BEGIN
  IF v < 1 THEN
    RAISE EXCEPTION 'нет доступа к файлу' USING ERRCODE = 'CM404';
  END IF;
  RETURN v;
END
$$;

-- Упоминания комментария: только люди с доступом к файлу сейчас. Уже заведённые (правка) остаются, если токен в тексте.
CREATE FUNCTION comment_mentions(p_file uuid, p_comment uuid, p_users text[]) RETURNS void
LANGUAGE sql AS $$
  DELETE FROM comment_mention WHERE comment_id = p_comment AND user_id <> ALL (p_users);
  INSERT INTO comment_mention (comment_id, user_id)
  SELECT p_comment, u.id FROM "user" u WHERE u.id = ANY (p_users) AND COALESCE(file_role_rank(p_file, u.id), 0) >= 1
  ON CONFLICT DO NOTHING;
$$;

-- Письма о комментарии (раздел 10.4): упомянутые с доступом и, у ответа, участники обсуждения — кроме автора, тех, у кого
-- файл открыт сейчас (`p_open`, соединения сервера), выключивших письма и потерявших доступ. Строка на получателя и файл
-- одна: новое сливается в ждущую, не больше 20 id и счётчик сверху; ждущих строк у человека — не больше 50 (ещё файл
-- сверх этого — без письма). Срок — от первого: группировка за `p_delay_ms`.
CREATE FUNCTION comment_enqueue(p_file uuid, p_thread uuid, p_comment uuid, p_author text, p_reply boolean, p_open text[], p_delay_ms integer)
RETURNS void
LANGUAGE sql AS $$
  INSERT INTO mail_outbox AS o (recipient_id, file_id, comment_ids, first_at, due_at)
  SELECT r.u, p_file, ARRAY[p_comment], clock_timestamp(), clock_timestamp() + make_interval(secs => p_delay_ms / 1000.0)
  FROM (
    SELECT m.user_id AS u FROM comment_mention m WHERE m.comment_id = p_comment
    UNION
    SELECT c.author_id FROM comment c WHERE p_reply AND c.thread_id = p_thread AND c.author_id IS NOT NULL
    UNION
    SELECT m.user_id FROM comment c JOIN comment_mention m ON m.comment_id = c.id WHERE p_reply AND c.thread_id = p_thread
  ) r
  WHERE r.u <> p_author
    AND r.u <> ALL (p_open)
    AND COALESCE((SELECT comment_mail FROM user_pref WHERE user_id = r.u), true)
    AND COALESCE(file_role_rank(p_file, r.u), 0) >= 1
    AND (EXISTS (SELECT 1 FROM mail_outbox q WHERE q.recipient_id = r.u AND q.file_id = p_file AND q.state = 'pending')
         OR (SELECT count(*) FROM mail_outbox q WHERE q.recipient_id = r.u AND q.state = 'pending') < 50)
  ON CONFLICT (recipient_id, file_id) WHERE state = 'pending' DO UPDATE SET
    comment_ids = CASE WHEN cardinality(o.comment_ids) < 20 THEN o.comment_ids || EXCLUDED.comment_ids ELSE o.comment_ids END,
    extra = o.extra + CASE WHEN cardinality(o.comment_ids) < 20 THEN 0 ELSE 1 END;
$$;

-- Пределы (`p_file_max`, `p_user_max`) — живые обсуждения файла и человека в нём, под блокировкой номера: два одновременных
-- создания не проходят оба сверх предела.
CREATE FUNCTION comment_create(
  p_file uuid, p_user text, p_page text, p_node text, p_x float8, p_y float8, p_wx float8, p_wy float8,
  p_body text, p_preview text, p_more boolean, p_mentions text[], p_open text[], p_delay_ms integer,
  p_file_max integer, p_user_max integer
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_thread uuid := gen_random_uuid();
  v_comment uuid := gen_random_uuid();
  v_seq bigint;
  v_at timestamptz;
BEGIN
  -- Строка файла — раньше номера, как у удаления файла (файл, потом каскад на счётчик): вставка обсуждения всё равно
  -- берёт её (внешний ключ), и в обратном порядке удаление файла и новое обсуждение ждали бы друг друга крест-накрест.
  PERFORM 1 FROM file WHERE id = p_file FOR KEY SHARE;
  v_seq := comment_next_seq(p_file);
  PERFORM comment_rank(p_file, p_user);
  IF (SELECT count(*) FROM (SELECT 1 FROM comment_thread WHERE file_id = p_file AND deleted_at IS NULL LIMIT p_file_max) x)
     >= p_file_max THEN
    RAISE EXCEPTION 'в файле слишком много обсуждений' USING ERRCODE = 'CMF09';
  END IF;
  IF (SELECT count(*) FROM (SELECT 1 FROM comment_thread
                            WHERE file_id = p_file AND created_by = p_user AND deleted_at IS NULL LIMIT p_user_max) x)
     >= p_user_max THEN
    RAISE EXCEPTION 'слишком много своих обсуждений в файле' USING ERRCODE = 'CMU09';
  END IF;
  -- После блокировки: время комментариев файла растёт в порядке номеров, прочтение по времени не пропустит соседа.
  v_at := clock_timestamp();
  INSERT INTO comment_thread (id, file_id, page_id, node_id, x, y, wx, wy, first_id, preview, preview_more, created_by, created_at, seq)
  VALUES (v_thread, p_file, p_page, p_node, p_x, p_y, p_wx, p_wy, v_comment, p_preview, p_more, p_user, v_at, v_seq);
  INSERT INTO comment (id, thread_id, author_id, body, created_at) VALUES (v_comment, v_thread, p_user, p_body, v_at);
  PERFORM comment_mentions(p_file, v_comment, p_mentions);
  PERFORM comment_thread_refresh(v_thread);
  INSERT INTO comment_read (thread_id, user_id, read_at) VALUES (v_thread, p_user, v_at);
  PERFORM comment_enqueue(p_file, v_thread, v_comment, p_user, false, p_open, p_delay_ms);
  RETURN comment_result(v_thread, p_user, v_comment, false, NULL);
END
$$;

CREATE FUNCTION comment_reply(
  p_file uuid, p_user text, p_thread uuid, p_body text, p_mentions text[], p_open text[], p_delay_ms integer, p_max integer,
  p_max_bytes integer
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_comment uuid := gen_random_uuid();
  v_seq bigint;
  v_at timestamptz;
BEGIN
  v_seq := comment_next_seq(p_file);
  PERFORM comment_rank(p_file, p_user);
  UPDATE comment_thread SET seq = v_seq WHERE id = p_thread AND file_id = p_file AND deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет обсуждения' USING ERRCODE = 'CM404';
  END IF;
  IF (SELECT count(*) FROM comment WHERE thread_id = p_thread) >= p_max
     OR (SELECT bytes FROM comment_thread WHERE id = p_thread) + octet_length(p_body) > p_max_bytes THEN
    RAISE EXCEPTION 'обсуждение заполнено' USING ERRCODE = 'CM409';
  END IF;
  v_at := clock_timestamp();
  INSERT INTO comment (id, thread_id, author_id, body, created_at) VALUES (v_comment, p_thread, p_user, p_body, v_at);
  PERFORM comment_mentions(p_file, v_comment, p_mentions);
  PERFORM comment_thread_refresh(p_thread);
  INSERT INTO comment_read (thread_id, user_id, read_at) VALUES (p_thread, p_user, v_at)
  ON CONFLICT (thread_id, user_id) DO UPDATE SET read_at = GREATEST(comment_read.read_at, EXCLUDED.read_at);
  PERFORM comment_enqueue(p_file, p_thread, v_comment, p_user, true, p_open, p_delay_ms);
  RETURN comment_result(p_thread, p_user, v_comment, false, NULL);
END
$$;

-- Правка: только свой комментарий **(поведение, как в Figma: без срока)**. Новые упоминания — с проверкой доступа, писем
-- правка не шлёт. Текст обсуждения не растёт сверх `p_max_bytes`; начало текста — у первого комментария.
CREATE FUNCTION comment_edit(
  p_file uuid, p_user text, p_thread uuid, p_comment uuid, p_body text, p_preview text, p_more boolean, p_mentions text[],
  p_max_bytes integer
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_seq bigint;
  v_author text;
  v_old integer;
  v_bytes integer;
BEGIN
  v_seq := comment_next_seq(p_file);
  PERFORM comment_rank(p_file, p_user);
  SELECT c.author_id, octet_length(c.body), t.bytes INTO v_author, v_old, v_bytes FROM comment c JOIN comment_thread t ON t.id = c.thread_id
  WHERE c.id = p_comment AND c.thread_id = p_thread AND t.file_id = p_file AND t.deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет комментария' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user THEN
    RAISE EXCEPTION 'чужой комментарий' USING ERRCODE = 'CM403';
  END IF;
  IF v_bytes - v_old + octet_length(p_body) > p_max_bytes THEN
    RAISE EXCEPTION 'обсуждение заполнено' USING ERRCODE = 'CM409';
  END IF;
  UPDATE comment SET body = p_body, edited_at = clock_timestamp() WHERE id = p_comment;
  PERFORM comment_mentions(p_file, p_comment, p_mentions);
  UPDATE comment_thread SET seq = v_seq,
    preview = CASE WHEN first_id = p_comment THEN p_preview ELSE preview END,
    preview_more = CASE WHEN first_id = p_comment THEN p_more ELSE preview_more END
  WHERE id = p_thread;
  PERFORM comment_thread_refresh(p_thread);
  RETURN comment_result(p_thread, p_user, p_comment, false, NULL);
END
$$;

-- Удаление: свой комментарий или любой — владельцу файла. Первый комментарий удаляет обсуждение для всех.
CREATE FUNCTION comment_delete(p_file uuid, p_user text, p_thread uuid, p_comment uuid) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_rank integer;
  v_seq bigint;
  v_author text;
  v_first uuid;
  v_result jsonb;
BEGIN
  v_seq := comment_next_seq(p_file);
  v_rank := comment_rank(p_file, p_user);
  SELECT c.author_id, t.first_id INTO v_author, v_first FROM comment c JOIN comment_thread t ON t.id = c.thread_id
  WHERE c.id = p_comment AND c.thread_id = p_thread AND t.file_id = p_file AND t.deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет комментария' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user AND v_rank < 3 THEN
    RAISE EXCEPTION 'чужой комментарий' USING ERRCODE = 'CM403';
  END IF;
  IF p_comment = v_first THEN
    UPDATE comment_thread SET seq = v_seq, deleted_at = clock_timestamp() WHERE id = p_thread;
    -- Последнее состояние — до удаления комментариев: ответ несёт обсуждение целиком.
    v_result := comment_result(p_thread, p_user, NULL, true, p_comment);
    DELETE FROM comment WHERE thread_id = p_thread;
    DELETE FROM comment_read WHERE thread_id = p_thread;
    RETURN v_result;
  END IF;
  DELETE FROM comment WHERE id = p_comment;
  UPDATE comment_thread SET seq = v_seq WHERE id = p_thread;
  PERFORM comment_thread_refresh(p_thread);
  RETURN comment_result(p_thread, p_user, NULL, false, p_comment);
END
$$;

-- Закрыть и открыть: автор обсуждения или редактор и выше **(поведение, как в Figma)**.
CREATE FUNCTION comment_resolve(p_file uuid, p_user text, p_thread uuid, p_resolved boolean) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_rank integer;
  v_seq bigint;
  v_author text;
BEGIN
  v_seq := comment_next_seq(p_file);
  v_rank := comment_rank(p_file, p_user);
  SELECT created_by INTO v_author FROM comment_thread WHERE id = p_thread AND file_id = p_file AND deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет обсуждения' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user AND v_rank < 2 THEN
    RAISE EXCEPTION 'не хватает роли' USING ERRCODE = 'CM403';
  END IF;
  UPDATE comment_thread SET
    seq = v_seq,
    resolved_at = CASE WHEN NOT p_resolved THEN NULL WHEN resolved_at IS NULL THEN clock_timestamp() ELSE resolved_at END,
    resolved_by = CASE WHEN NOT p_resolved THEN NULL WHEN resolved_at IS NULL THEN p_user ELSE resolved_by END
  WHERE id = p_thread;
  RETURN comment_result(p_thread, p_user, NULL, false, NULL);
END
$$;

-- Перенос метки: автор обсуждения или владелец файла (раздел 10.3).
CREATE FUNCTION comment_move(
  p_file uuid, p_user text, p_thread uuid, p_page text, p_node text, p_x float8, p_y float8, p_wx float8, p_wy float8
) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE
  v_rank integer;
  v_seq bigint;
  v_author text;
BEGIN
  v_seq := comment_next_seq(p_file);
  v_rank := comment_rank(p_file, p_user);
  SELECT created_by INTO v_author FROM comment_thread WHERE id = p_thread AND file_id = p_file AND deleted_at IS NULL;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'нет обсуждения' USING ERRCODE = 'CM404';
  END IF;
  IF v_author IS DISTINCT FROM p_user AND v_rank < 3 THEN
    RAISE EXCEPTION 'не хватает роли' USING ERRCODE = 'CM403';
  END IF;
  UPDATE comment_thread SET seq = v_seq, page_id = p_page, node_id = p_node, x = p_x, y = p_y, wx = p_wx, wy = p_wy
  WHERE id = p_thread;
  RETURN comment_result(p_thread, p_user, NULL, false, NULL);
END
$$;
