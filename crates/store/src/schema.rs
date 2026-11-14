//! 열기와 이전(D6). 스키마 버전은 SQLite의 `user_version`에 둔다.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use crate::SCHEMA_VERSION;
use crate::error::StoreError;
use crate::model::StoreInfo;

/// 이 저장소가 만든 파일이라는 표시(`application_id`). 글자 "PIST".
const APPLICATION_ID: i64 = 0x5049_5354;

/// 다른 연결이 쓰는 중일 때 기다리는 시간.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// 버전 n에서 n+1로 올리는 SQL. `MIGRATIONS[n]`이 n → n+1이다. 이미 나간 것은 고치지 않고 뒤에 더한다.
const MIGRATIONS: [&str; SCHEMA_VERSION as usize] = [V1];

/// 기록 표(team_history · events · access_log)는 고치기 · 지우기를 SQLite가 막는다.
const V1: &str = "
CREATE TABLE team_history (
    seq            INTEGER PRIMARY KEY,
    at             INTEGER NOT NULL,
    from_state     TEXT    NOT NULL,
    to_state       TEXT    NOT NULL,
    cause          TEXT    NOT NULL,
    detail         TEXT    NOT NULL,
    owner_approved INTEGER NOT NULL
);
CREATE TABLE trades (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol               TEXT    NOT NULL,
    account              TEXT    NOT NULL,
    state                TEXT    NOT NULL,
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL,
    hypothesis           TEXT,
    trigger_note         TEXT,
    entry                INTEGER,
    stop                 INTEGER,
    target               INTEGER,
    expected_value       INTEGER,
    decided_at           INTEGER,
    consensus_score      INTEGER,
    approval_number      INTEGER,
    approval_fingerprint TEXT
);
CREATE INDEX trades_by_state ON trades (state, id);
CREATE TABLE trade_sources (
    trade_id INTEGER NOT NULL REFERENCES trades (id),
    pos      INTEGER NOT NULL,
    name     TEXT    NOT NULL,
    known_at INTEGER NOT NULL,
    PRIMARY KEY (trade_id, pos)
);
CREATE TABLE trade_scores (
    trade_id INTEGER NOT NULL REFERENCES trades (id),
    pos      INTEGER NOT NULL,
    role     TEXT    NOT NULL,
    score    INTEGER NOT NULL,
    reason   TEXT    NOT NULL,
    PRIMARY KEY (trade_id, pos)
);
CREATE TABLE events (
    seq     INTEGER PRIMARY KEY,
    kind    TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE access_log (
    seq    INTEGER PRIMARY KEY,
    at     INTEGER NOT NULL,
    origin TEXT    NOT NULL,
    action TEXT    NOT NULL
);
CREATE TRIGGER team_history_no_update BEFORE UPDATE ON team_history
BEGIN SELECT RAISE(ABORT, 'team_history is append-only'); END;
CREATE TRIGGER team_history_no_delete BEFORE DELETE ON team_history
BEGIN SELECT RAISE(ABORT, 'team_history is append-only'); END;
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
";

/// 파일을 열고, 이 저장소의 파일인지 본 뒤, 설정을 켜고 스키마를 최신으로 올린다.
pub(crate) fn open(path: &Path) -> Result<Connection, StoreError> {
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // 파일을 건드리기 전에 먼저 읽어 본다. SQLite 파일이 아니면 여기서 오류가 난다.
    let version = check_ownership(&conn)?;
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "wal", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::Incompatible(format!(
            "WAL을 켤 수 없다(journal_mode={mode})"
        )));
    }
    conn.pragma_update(None, "foreign_keys", true)?;
    if version < SCHEMA_VERSION {
        migrate(&mut conn)?;
    }
    Ok(conn)
}

/// 열린 저장소의 스키마 버전과 설정을 읽는다.
pub(crate) fn info(conn: &Connection) -> Result<StoreInfo, StoreError> {
    let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let foreign_keys: bool = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    Ok(StoreInfo {
        schema_version: user_version(conn)?,
        journal_mode: journal_mode.to_lowercase(),
        foreign_keys,
    })
}

fn user_version(conn: &Connection) -> Result<u32, StoreError> {
    let raw: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    u32::try_from(raw)
        .map_err(|_| StoreError::Incompatible(format!("스키마 버전이 이상하다: {raw}")))
}

/// 이 저장소가 쓸 수 있는 파일인지 보고 지금 스키마 버전을 준다. 아무것도 바꾸지 않는다.
///
/// - 버전 0: 빈 파일이어야 한다. 표가 있으면 다른 프로그램의 파일이다.
/// - 버전 1 이상: 이 저장소의 표시가 있어야 하고, 아는 버전보다 새것이면 안 된다.
fn check_ownership(conn: &Connection) -> Result<u32, StoreError> {
    let version = user_version(conn)?;
    if version == 0 {
        let objects: i64 =
            conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))?;
        if objects > 0 {
            return Err(StoreError::Incompatible(
                "이 저장소가 만든 파일이 아니다(다른 표가 이미 있다)".to_string(),
            ));
        }
        return Ok(version);
    }
    let app_id: i64 = conn.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if app_id != APPLICATION_ID {
        return Err(StoreError::Incompatible(
            "이 저장소가 만든 파일이 아니다(표시가 다르다)".to_string(),
        ));
    }
    if version > SCHEMA_VERSION {
        return Err(StoreError::Incompatible(format!(
            "파일의 스키마 버전 {version}이 이 프로그램이 아는 {SCHEMA_VERSION}보다 새것이다"
        )));
    }
    Ok(version)
}

/// 지금 버전에서 [`SCHEMA_VERSION`]까지 한 단계씩 올린다. 전부 되거나 전부 안 된다.
fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // 쓰기 잠금을 잡은 뒤 다시 본다(그 사이 다른 연결이 올렸을 수 있다).
    let version = check_ownership(&tx)?;
    for step in MIGRATIONS.iter().skip(version as usize) {
        tx.execute_batch(step)?;
    }
    if version < SCHEMA_VERSION {
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi.sqlite");
        (dir, path)
    }

    #[test]
    fn 더_새_스키마의_파일은_열지_않는다() {
        let (_dir, path) = temp_path();
        drop(open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(conn);
        assert!(matches!(open(&path), Err(StoreError::Incompatible(_))));
    }

    #[test]
    fn 다른_프로그램의_sqlite_파일은_열지_않고_바꾸지도_않는다() {
        let (_dir, path) = temp_path();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('x');")
            .unwrap();
        drop(conn);
        assert!(matches!(open(&path), Err(StoreError::Incompatible(_))));

        let conn = Connection::open(&path).unwrap();
        let tables: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tables, 1);
        let mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_ne!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn 표시가_다른_파일은_버전이_맞아도_열지_않는다() {
        let (_dir, path) = temp_path();
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        drop(conn);
        assert!(matches!(open(&path), Err(StoreError::Incompatible(_))));
    }

    #[test]
    fn 기록_표는_sql로도_고치거나_지울_수_없다() {
        let (_dir, path) = temp_path();
        let conn = open(&path).unwrap();
        conn.execute(
            "INSERT INTO events (seq, kind, payload) VALUES (1, 'chat', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO access_log (seq, at, origin, action) VALUES (1, 1, 'a', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO team_history VALUES (1, 1, 'resting', 'operating', 'trading_start', '', 0)",
            [],
        )
        .unwrap();
        for table in ["events", "access_log", "team_history"] {
            assert!(conn.execute(&format!("DELETE FROM {table}"), []).is_err());
            assert!(
                conn.execute(&format!("UPDATE {table} SET seq = seq + 10"), [])
                    .is_err()
            );
            let rows: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(rows, 1);
        }
    }
}
