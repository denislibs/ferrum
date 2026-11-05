-- Файлы на сервере (спека подсистемы 4, раздел 6.1). Сверх модели спеки: поколение и размер
-- содержимого, номер последней записи, ключ содержимого, версия превью; пометка «кто убрал в корзину»;
-- открытия по пользователям; приглашения по адресам без учётной записи; список байтов к удалению.

CREATE TABLE team (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE team_member (
  id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
  team_id uuid NOT NULL REFERENCES team ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  role text NOT NULL CHECK (role IN ('owner', 'member')),
  PRIMARY KEY (team_id, user_id)
);
CREATE INDEX team_member_user_idx ON team_member (user_id);

CREATE TABLE team_invite (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  team_id uuid NOT NULL REFERENCES team ON DELETE CASCADE,
  email text NOT NULL,
  invited_by text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (team_id, email)
);
CREATE INDEX team_invite_email_idx ON team_invite (email);

CREATE TABLE project (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  team_id uuid NOT NULL REFERENCES team ON DELETE CASCADE,
  name text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX project_team_idx ON project (team_id);

CREATE TABLE file (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  owner_id text NOT NULL REFERENCES "user",
  -- Пусто — «Черновики» владельца.
  project_id uuid REFERENCES project ON DELETE SET NULL,
  name text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  edited_at timestamptz NOT NULL DEFAULT now(),
  trashed_at timestamptz,
  trashed_by text REFERENCES "user",
  thumb_key text,
  thumb_version integer NOT NULL DEFAULT 0,
  -- Заполнит этап 3 (`hello { schema }`): сервер этапа 2 документ не раскодирует.
  schema_version integer NOT NULL DEFAULT 1,
  -- Поколение содержимого: растёт с каждой записью (раздел 6.5).
  version bigint NOT NULL DEFAULT 0,
  size bigint NOT NULL DEFAULT 0,
  content_key text,
  last_write_id uuid
);
CREATE INDEX file_owner_drafts_idx ON file (owner_id, edited_at DESC) WHERE project_id IS NULL AND trashed_at IS NULL;
CREATE INDEX file_project_idx ON file (project_id, edited_at DESC) WHERE trashed_at IS NULL;
CREATE INDEX file_trash_idx ON file (trashed_at DESC) WHERE trashed_at IS NOT NULL;

CREATE TABLE file_access (
  id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  role text NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
  PRIMARY KEY (file_id, user_id)
);
CREATE INDEX file_access_user_idx ON file_access (user_id);

CREATE TABLE file_invite (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  email text NOT NULL,
  role text NOT NULL CHECK (role IN ('editor', 'viewer')),
  invited_by text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (file_id, email)
);
CREATE INDEX file_invite_email_idx ON file_invite (email);

-- Токен — только хэшем. `nonce` нужен, чтобы показать ссылку снова: токен выводится из секрета сервера.
CREATE TABLE share_link (
  file_id uuid PRIMARY KEY REFERENCES file ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,
  nonce text NOT NULL,
  role text NOT NULL CHECK (role IN ('editor', 'viewer')),
  enabled boolean NOT NULL
);

-- Кто пришёл по ссылке: роль по ссылке действует, пока ссылка включена.
CREATE TABLE file_link_user (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  PRIMARY KEY (file_id, user_id)
);
CREATE INDEX file_link_user_user_idx ON file_link_user (user_id);

CREATE TABLE file_recent (
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  opened_at timestamptz NOT NULL,
  PRIMARY KEY (user_id, file_id)
);
CREATE INDEX file_recent_order_idx ON file_recent (user_id, opened_at DESC);

CREATE TABLE image (
  hash text PRIMARY KEY,
  size bigint NOT NULL,
  blob_key text NOT NULL,
  -- С этого момента картинка не нужна ни одному файлу.
  orphaned_at timestamptz
);

CREATE TABLE file_image (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  hash text NOT NULL REFERENCES image,
  -- С этого момента на неё нет ссылок в содержимом файла (отмена может её вернуть). Новая ссылка
  -- получает срок сразу: загруженная и ни разу не использованная картинка тоже удаляется.
  unreferenced_at timestamptz DEFAULT now(),
  PRIMARY KEY (file_id, hash)
);
CREATE INDEX file_image_hash_idx ON file_image (hash);

-- Байты к удалению: запись появляется в той же транзакции, что и замена. Сбой между фиксацией и
-- удалением ничего не теряет, а читатель, начавший раньше, успевает дочитать.
CREATE TABLE blob_trash (
  key text PRIMARY KEY,
  delete_after timestamptz NOT NULL
);

-- Загрузки, ещё не зафиксированные в базе: запись появляется до первого байта и снимается той же
-- транзакцией, что и фиксация. Сбой транзакции или процесса оставляет запись, очистка по сроку
-- переносит ключ в `blob_trash`. Перенос и фиксация спорят за одну строку: выигрывает одна из них.
CREATE TABLE blob_pending (
  key text PRIMARY KEY,
  started_at timestamptz NOT NULL DEFAULT now()
);

-- Действующая роль (раздел 6.1): наибольшая из прямой, по ссылке и через команду. 0 — нет доступа.
-- Участник команды — редактор на файлах её проектов, владелец команды — владелец.
-- Файл в корзине виден только владельцу и тому, кто его туда убрал.
CREATE FUNCTION file_role_rank(p_file uuid, p_user text) RETURNS integer
LANGUAGE sql STABLE AS $$
  SELECT CASE
    WHEN f.trashed_at IS NOT NULL AND f.trashed_by = p_user THEN 3
    WHEN f.trashed_at IS NOT NULL AND f.owner_id <> p_user THEN 0
    ELSE GREATEST(
      COALESCE((SELECT CASE a.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END
                FROM file_access a WHERE a.file_id = f.id AND a.user_id = p_user), 0),
      COALESCE((SELECT CASE m.role WHEN 'owner' THEN 3 ELSE 2 END
                FROM project p JOIN team_member m ON m.team_id = p.team_id AND m.user_id = p_user
                WHERE p.id = f.project_id), 0),
      COALESCE((SELECT CASE l.role WHEN 'editor' THEN 2 ELSE 1 END
                FROM share_link l JOIN file_link_user u ON u.file_id = l.file_id AND u.user_id = p_user
                WHERE l.file_id = f.id AND l.enabled), 0)
    )
  END
  FROM file f WHERE f.id = p_file
$$;
