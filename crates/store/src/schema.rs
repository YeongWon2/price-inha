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
const MIGRATIONS: [&str; SCHEMA_VERSION as usize] = [V1, V2];

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

/// 버전 2(2차 작업). 버전 1의 표와 줄은 그대로 두고 더하기만 한다.
///
/// - 매매에 막힌 이유 칸 둘. 버전 1에서 막힌 매매는 이유가 비어 있다.
/// - 한 번이라도 매매에 붙은 승인 번호의 목록(`approval_numbers`). 번호의 주인은 처음 붙인 매매이고
///   바뀌거나 지워지지 않는다. 버전 1 파일에 같은 번호가 여러 매매에 붙어 있으면 번호가 가장 작은
///   매매가 주인이 된다(매매 줄은 고치지 않는다).
/// - 기록 표에 "이미 있는 번호로 넣기"를 막는 트리거. `INSERT OR REPLACE`는 지우기 트리거를
///   부르지 않고 줄을 갈아 끼우므로, 넣기 전에 같은 번호가 있는지 보고 막는다.
///   기록 표의 줄은 건드리지 않으므로 버전 1의 보호 트리거와 부딪히지 않는다.
const V2: &str = "
ALTER TABLE trades ADD COLUMN block_kind TEXT;
ALTER TABLE trades ADD COLUMN block_detail TEXT;
CREATE TABLE approval_numbers (
    number   INTEGER PRIMARY KEY,
    trade_id INTEGER NOT NULL REFERENCES trades (id)
);
INSERT OR IGNORE INTO approval_numbers (number, trade_id)
SELECT approval_number, id FROM trades WHERE approval_number IS NOT NULL ORDER BY id;
CREATE TRIGGER approval_numbers_no_replace BEFORE INSERT ON approval_numbers
WHEN EXISTS (SELECT 1 FROM approval_numbers WHERE number = NEW.number)
BEGIN SELECT RAISE(ABORT, 'approval number is already used'); END;
CREATE TRIGGER approval_numbers_no_update BEFORE UPDATE ON approval_numbers
BEGIN SELECT RAISE(ABORT, 'approval_numbers is append-only'); END;
CREATE TRIGGER approval_numbers_no_delete BEFORE DELETE ON approval_numbers
BEGIN SELECT RAISE(ABORT, 'approval_numbers is append-only'); END;
CREATE TRIGGER team_history_no_replace BEFORE INSERT ON team_history
WHEN EXISTS (SELECT 1 FROM team_history WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'team_history is append-only'); END;
CREATE TRIGGER events_no_replace BEFORE INSERT ON events
WHEN EXISTS (SELECT 1 FROM events WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER access_log_no_replace BEFORE INSERT ON access_log
WHEN EXISTS (SELECT 1 FROM access_log WHERE seq = NEW.seq)
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

    /// 버전 1 스키마만 가진 파일을 만든다.
    fn v1_file(path: &std::path::Path, extra_sql: &str) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(V1).unwrap();
        conn.execute_batch(extra_sql).unwrap();
        conn.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    #[test]
    fn 기록_표는_같은_번호로_다시_넣어도_바뀌지_않는다() {
        let (_dir, path) = temp_path();
        let conn = open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO events (seq, kind, payload) VALUES (1, 'chat', '{}');
             INSERT INTO access_log (seq, at, origin, action) VALUES (1, 1, 'a', 'b');
             INSERT INTO team_history VALUES (1, 1, 'resting', 'operating', 'trading_start', '', 0);",
        )
        .unwrap();
        for sql in [
            "INSERT OR REPLACE INTO events (seq, kind, payload) VALUES (1, 'x', '{}')",
            "REPLACE INTO events (rowid, kind, payload) VALUES ('1', 'x', '{}')",
            "INSERT OR REPLACE INTO access_log (seq, at, origin, action) VALUES (1.0, 9, 'x', 'x')",
            "REPLACE INTO team_history VALUES (1, 9, 'resting', 'resting', 'market_close', 'x', 1)",
        ] {
            assert!(conn.execute(sql, []).is_err(), "{sql}");
        }
        let kind: String = conn
            .query_row("SELECT kind FROM events WHERE seq = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kind, "chat");
        // 번호를 정하지 않고 넣는 것과 새 번호로 넣는 것은 된다.
        conn.execute(
            "INSERT INTO events (kind, payload) VALUES ('chat', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events (seq, kind, payload) VALUES (3, 'chat', '{}')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn 버전_1에서_같은_승인_번호가_두_매매에_있어도_이전이_되고_매매는_그대로다() {
        let (_dir, path) = temp_path();
        v1_file(
            &path,
            "INSERT INTO trades (symbol, account, state, created_at, updated_at,
                 approval_number, approval_fingerprint)
             VALUES ('A', 'paper', 'judged', 1, 1, 7, 'fa'), ('B', 'paper', 'judged', 1, 1, 7, 'fb');",
        );
        let conn = open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);
        let owner: i64 = conn
            .query_row(
                "SELECT trade_id FROM approval_numbers WHERE number = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(owner, 1);
        let kept: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM trades WHERE approval_number = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, 2);
    }

    #[test]
    fn 이전이_중간에_실패하면_버전_1_그대로_남는다() {
        let (_dir, path) = temp_path();
        // 버전 2가 만들 표 이름을 미리 차지해 이전이 중간(칸을 더한 뒤)에 실패하게 한다.
        v1_file(&path, "CREATE TABLE approval_numbers (x);");
        assert!(open(&path).is_err());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 1);
        let added: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('trades') WHERE name = 'block_kind'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(added, 0);
    }

    #[test]
    fn 승인_번호_목록은_고치거나_지울_수_없다() {
        let (_dir, path) = temp_path();
        let conn = open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO trades (symbol, account, state, created_at, updated_at)
                 VALUES ('A', 'paper', 'judged', 1, 1), ('B', 'paper', 'judged', 1, 1);
             INSERT INTO approval_numbers (number, trade_id) VALUES (7, 1);",
        )
        .unwrap();
        for sql in [
            "UPDATE approval_numbers SET trade_id = 2",
            "DELETE FROM approval_numbers",
            "INSERT OR REPLACE INTO approval_numbers (number, trade_id) VALUES (7, 2)",
        ] {
            assert!(conn.execute(sql, []).is_err(), "{sql}");
        }
    }
}
