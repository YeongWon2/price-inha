//! 열기와 이전(D6). 스키마 버전은 SQLite의 `user_version`에 둔다.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use crate::SCHEMA_VERSION;
use crate::error::StoreError;
use crate::model::StoreInfo;

mod perm;
mod read_only;
mod v1;
mod v10;
mod v11;
mod v12;
mod v2;
mod v3;
mod v4;
mod v5;
mod v6;
mod v7;
mod v8;
mod v9;

use v1::V1;
use v2::V2;
use v3::V3;
use v4::V4;
use v5::V5;
use v6::V6;
use v7::V7;
use v8::V8;
use v9::V9;
use v10::V10;
use v11::V11;
use v12::V12;

pub(crate) use v4::{legacy_envelope_seqs, legacy_envelope_seqs_upto, legacy_envelope_until};

pub(crate) use read_only::{integrity_ok, open_read_only};

/// 이 저장소가 만든 파일이라는 표시(`application_id`). 글자 "PIST".
const APPLICATION_ID: i64 = 0x5049_5354;

/// 다른 연결이 쓰는 중일 때 기다리는 시간.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// 버전 n에서 n+1로 올리는 SQL. `MIGRATIONS[n]`이 n → n+1이다. 이미 나간 것은 고치지 않고 뒤에 더한다.
const MIGRATIONS: [&str; SCHEMA_VERSION as usize] =
    [V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12];

/// 파일을 열고, 이 저장소의 파일인지 본 뒤, 설정을 켜고 스키마를 최신으로 올린다.
///
/// 저장 파일 권한([`perm`]): 연결 전에는 없는 파일만 0600으로 만든다. 이미 있는 파일은
/// 이 저장소의 파일임을 확인한 뒤에야 좁히고(거부한 파일은 권한도 그대로다), WAL을 켠 뒤 한 번 더 맞춘다.
pub(crate) fn open(path: &Path) -> Result<Connection, StoreError> {
    perm::create_if_missing(path)?;
    let mut conn = Connection::open(path)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // 파일을 건드리기 전에 먼저 읽어 본다. SQLite 파일이 아니면 여기서 오류가 난다.
    let version = check_ownership(&conn)?;
    perm::tighten(path)?;
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "wal", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::Incompatible(format!(
            "WAL을 켤 수 없다(journal_mode={mode})"
        )));
    }
    perm::tighten(path)?;
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
    // 경계 번호는 시각 칸을 더하고 옮겨 적기 전의 모양으로 정한다(버전 4 단계 문서).
    let boundary = if version < v4::VERSION {
        Some(v4::boundary_before(&tx, version)?)
    } else {
        None
    };
    for (index, step) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        if index + 1 == 10 {
            v10::relax_template_check(&tx)?;
        }
        tx.execute_batch(step)?;
        if index + 1 == v12::VERSION as usize {
            v12::install_delete_guards(&tx)?;
        }
    }
    if let Some(boundary) = boundary {
        v4::seal(&tx, boundary)?;
    }
    if version < SCHEMA_VERSION {
        v11::integrity_ok(&tx)?;
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
