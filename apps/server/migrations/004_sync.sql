-- Синхронизация (спека подсистемы 4, разделы 7.3 и 7.4): журнал обновлений по документам и их состояние.

CREATE TABLE doc_state (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  -- `root` или `TreeID` страницы.
  doc text NOT NULL,
  -- Следующий номер записи журнала этого документа.
  next_seq bigint NOT NULL DEFAULT 1,
  -- Вектор версий сервера: снимок и журнал вместе.
  vv jsonb NOT NULL DEFAULT '{}',
  -- Что вошло в снимок: подписка не покрывает его — клиенту нужен снимок.
  base_vv jsonb NOT NULL DEFAULT '{}',
  log_count integer NOT NULL DEFAULT 0,
  log_bytes bigint NOT NULL DEFAULT 0,
  PRIMARY KEY (file_id, doc)
);

CREATE TABLE doc_update (
  file_id uuid NOT NULL,
  doc text NOT NULL,
  seq bigint NOT NULL,
  start_vv jsonb NOT NULL,
  end_vv jsonb NOT NULL,
  -- Байты Loro уже плотные, сжимать их базе незачем.
  bytes bytea NOT NULL,
  PRIMARY KEY (file_id, doc, seq),
  FOREIGN KEY (file_id, doc) REFERENCES doc_state ON DELETE CASCADE
);
ALTER TABLE doc_update ALTER COLUMN bytes SET STORAGE EXTERNAL;

-- Чей пир: пачка от чужого пира подделала бы чужие правки и заняла бы их счётчики.
CREATE TABLE doc_peer (
  file_id uuid NOT NULL REFERENCES file ON DELETE CASCADE,
  peer numeric(20) NOT NULL,
  user_id text NOT NULL REFERENCES "user" ON DELETE CASCADE,
  PRIMARY KEY (file_id, peer)
);

-- Сжатие не удалось: следующая попытка не раньше этого времени (раздел 7.4).
ALTER TABLE file ADD COLUMN compact_after timestamptz;
