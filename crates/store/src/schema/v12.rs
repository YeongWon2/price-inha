//! 버전 11에서 12로 올린다(21차, 결정 31 — 오래된 기록 덜어 내기).
//!
//! - `archive_batches`: 보관 묶음(요약 한 행). 쌓기만 한다(고치기 · 키가 겹치는 넣기 거부). 지우기 막는 트리거는
//!   두지 않는다(판 10의 새 표와 같은 까닭: 잠긴 시험 `round10_replace`가 지우기 막는 표를 자기 목록으로 센다.
//!   묶음 줄을 지워도 지우기 길은 좁아질 뿐 넓어지지 않는다).
//! - `events_no_delete` · `room_events_no_delete`를 다시 만든다: 보관 묶음이 덮는 줄(묶음의 종류 · 기간 ·
//!   rowid 이하)만 지울 수 있고, 표의 마지막 줄(이벤트: 가장 큰 번호, 방 로그: 방마다 가장 큰 번호)과 시각 없는 줄은
//!   언제나 막는다. 그 밖의 지우기는 지금처럼 막힌다. 감사 기록의 표 · 종류는 어느 묶음에도 들지 않는다.
//! - `knowledge_embeddings_q8`: 정수 양자화 임베딩(길이 = 차원 + 4). 조각을 지우면 함께 지워진다.

use rusqlite::Connection;

use crate::archive::ArchiveCategory;
use crate::error::StoreError;

/// 판 번호.
pub(super) const VERSION: u32 = 12;

/// 버전 11에서 12로 올리는 SQL(표). 지우기 트리거는 [`install_delete_guards`]가 종류 목록으로 만든다.
pub(super) const V12: &str = "
CREATE TABLE archive_batches (
    id          INTEGER PRIMARY KEY,
    category    TEXT    NOT NULL CHECK (category IN ('room', 'screen', 'ai_usage')),
    period      TEXT    NOT NULL CHECK (length(period) > 0),
    from_at     INTEGER NOT NULL,
    to_at       INTEGER NOT NULL,
    upto_rowid  INTEGER NOT NULL CHECK (upto_rowid > 0),
    row_count   INTEGER NOT NULL CHECK (row_count > 0),
    fingerprint TEXT    NOT NULL CHECK (length(fingerprint) = 16),
    file        TEXT    NOT NULL CHECK (length(file) > 0 AND instr(file, '/') = 0),
    file_sha256 TEXT    NOT NULL CHECK (length(file_sha256) = 64),
    summary     TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    CHECK (from_at < to_at AND to_at <= created_at),
    UNIQUE (category, period)
);
CREATE TRIGGER archive_batches_no_replace BEFORE INSERT ON archive_batches
WHEN EXISTS (SELECT 1 FROM archive_batches
             WHERE id = NEW.id OR (category = NEW.category AND period = NEW.period))
BEGIN SELECT RAISE(ABORT, 'archive_batches is append-only'); END;
CREATE TRIGGER archive_batches_no_update BEFORE UPDATE ON archive_batches
BEGIN SELECT RAISE(ABORT, 'archive_batches is append-only'); END;

CREATE TABLE knowledge_embeddings_q8 (
    chunk_id  INTEGER PRIMARY KEY REFERENCES knowledge_chunks (id),
    model     TEXT    NOT NULL,
    dims      INTEGER NOT NULL CHECK (dims > 0),
    embedding BLOB    NOT NULL,
    CHECK (length(embedding) = dims + 4)
);
CREATE TRIGGER knowledge_chunks_q8_delete AFTER DELETE ON knowledge_chunks
BEGIN
    DELETE FROM knowledge_embeddings_q8 WHERE chunk_id = OLD.id;
END;
CREATE TRIGGER knowledge_chunks_q8_update AFTER UPDATE ON knowledge_chunks
WHEN NEW.id <> OLD.id
BEGIN
    UPDATE knowledge_embeddings_q8 SET chunk_id = NEW.id WHERE chunk_id = OLD.id;
END;
";

/// `('a', 'b')` 모양의 SQL 목록(종류 이름은 이 저장소의 상수다).
fn sql_list(kinds: &[&str]) -> String {
    let quoted: Vec<String> = kinds.iter().map(|k| format!("'{k}'")).collect();
    format!("({})", quoted.join(", "))
}

/// 이벤트 표의 묶음 조건(묶음 `b`가 줄 `OLD`를 덮는가).
fn events_cover() -> String {
    let parts: Vec<String> = ArchiveCategory::ALL
        .iter()
        .filter(|c| c.table() == "events")
        .map(|c| {
            format!(
                "(b.category = '{}' AND OLD.kind IN {})",
                c.code(),
                sql_list(c.kinds())
            )
        })
        .collect();
    parts.join(" OR ")
}

/// 지우기 막는 트리거를 좁은 길로 다시 만든다(옛 것을 지우고 같은 이름으로).
pub(super) fn install_delete_guards(tx: &Connection) -> Result<(), StoreError> {
    let room = ArchiveCategory::Room;
    let sql = format!(
        "DROP TRIGGER events_no_delete;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
WHEN OLD.at IS NULL
  OR OLD.seq = (SELECT MAX(seq) FROM events)
  OR NOT EXISTS (SELECT 1 FROM archive_batches b
                 WHERE OLD.rowid <= b.upto_rowid AND OLD.at >= b.from_at AND OLD.at < b.to_at
                   AND ({events}))
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
DROP TRIGGER room_events_no_delete;
CREATE TRIGGER room_events_no_delete BEFORE DELETE ON room_events
WHEN OLD.seq = (SELECT MAX(seq) FROM room_events WHERE room = OLD.room)
  OR OLD.kind NOT IN {room_kinds}
  OR NOT EXISTS (SELECT 1 FROM archive_batches b
                 WHERE b.category = '{room_code}' AND OLD.rowid <= b.upto_rowid
                   AND OLD.at >= b.from_at AND OLD.at < b.to_at)
BEGIN SELECT RAISE(ABORT, 'room_events is append-only'); END;",
        events = events_cover(),
        room_kinds = sql_list(room.kinds()),
        room_code = room.code(),
    );
    tx.execute_batch(&sql)?;
    Ok(())
}
