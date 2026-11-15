//! 버전 6 → 7 (11차): 지식(조각 · FTS5 trigram · 임베딩 · 메타)과 웹 푸시 구독.
//!
//! 표가 스스로 지키는 것(어떤 SQL로도):
//! - 검색 색인(`knowledge_fts`)은 조각표의 넣기 · 고치기 · 지우기를 트리거가 따라가 어긋나지 않는다.
//! - 조각의 임베딩은 조각을 지우면(외래 키 설정이 꺼진 연결에서도) 함께 지워진다.
//! - 조각 번호가 겹치는 `INSERT OR REPLACE`(`rowid` 명시 포함)는 거부된다: 대체는 옛 줄을 지우면서
//!   지우기 트리거를 돌리지 않으므로 색인이 어긋난다.
//! - 되살리기 표(`restore_marks`)는 쌓기만 한다(고치기 · 지우기 · 대체 거부: 표시를 지우면 되살리기 전 승인이 다시 살아난다).
//! - 조각 번호는 `AUTOINCREMENT`라 지워도 다시 쓰지 않는다.

/// 버전 6에서 7로 올리는 SQL.
pub(super) const V7: &str = "
CREATE TABLE knowledge_chunks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT    NOT NULL
        CHECK (kind IN ('trade', 'review', 'mistake', 'market_note')),
    source     TEXT    NOT NULL
        CHECK (source IN ('mine', 'team', 'external')),
    record_ref TEXT    NOT NULL CHECK (record_ref <> ''),
    ordinal    INTEGER NOT NULL CHECK (ordinal >= 0),
    text       TEXT    NOT NULL CHECK (text <> ''),
    updated_at INTEGER NOT NULL,
    UNIQUE (kind, record_ref, ordinal)
);
CREATE VIRTUAL TABLE knowledge_fts USING fts5(text, tokenize = 'trigram');
CREATE TRIGGER knowledge_chunks_no_replace BEFORE INSERT ON knowledge_chunks
WHEN EXISTS (SELECT 1 FROM knowledge_chunks WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'knowledge_chunks rows are replaced by re-indexing'); END;
CREATE TRIGGER knowledge_chunks_index_insert AFTER INSERT ON knowledge_chunks
BEGIN
    INSERT INTO knowledge_fts (rowid, text) VALUES (NEW.id, NEW.text);
END;
CREATE TRIGGER knowledge_chunks_index_update AFTER UPDATE ON knowledge_chunks
BEGIN
    DELETE FROM knowledge_fts WHERE rowid = OLD.id;
    INSERT INTO knowledge_fts (rowid, text) VALUES (NEW.id, NEW.text);
    UPDATE knowledge_embeddings SET chunk_id = NEW.id WHERE chunk_id = OLD.id AND NEW.id <> OLD.id;
END;
CREATE TRIGGER knowledge_chunks_index_delete AFTER DELETE ON knowledge_chunks
BEGIN
    DELETE FROM knowledge_fts WHERE rowid = OLD.id;
    DELETE FROM knowledge_embeddings WHERE chunk_id = OLD.id;
END;

CREATE TABLE knowledge_embeddings (
    chunk_id  INTEGER PRIMARY KEY REFERENCES knowledge_chunks (id),
    model     TEXT    NOT NULL,
    dims      INTEGER NOT NULL CHECK (dims > 0),
    embedding BLOB    NOT NULL,
    CHECK (length(embedding) = dims * 4)
);

CREATE TABLE knowledge_meta (
    key   TEXT PRIMARY KEY CHECK (key <> ''),
    value TEXT NOT NULL
);

CREATE TABLE restore_marks (
    seq INTEGER PRIMARY KEY,
    at  INTEGER NOT NULL
);
CREATE TRIGGER restore_marks_no_replace BEFORE INSERT ON restore_marks
WHEN EXISTS (SELECT 1 FROM restore_marks WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'restore_marks is append-only'); END;
CREATE TRIGGER restore_marks_no_update BEFORE UPDATE ON restore_marks
BEGIN SELECT RAISE(ABORT, 'restore_marks is append-only'); END;
CREATE TRIGGER restore_marks_no_delete BEFORE DELETE ON restore_marks
BEGIN SELECT RAISE(ABORT, 'restore_marks is append-only'); END;

CREATE TABLE push_subscriptions (
    endpoint   TEXT    PRIMARY KEY CHECK (endpoint LIKE 'https://%'),
    p256dh     TEXT    NOT NULL CHECK (p256dh <> ''),
    auth       TEXT    NOT NULL CHECK (auth <> ''),
    device     TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
";
