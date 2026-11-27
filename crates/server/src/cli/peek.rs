//! 서버를 띄우지 않는 명령이 저장소를 **읽기만** 하는 길(10차 10a 0번).
//!
//! `pi_store::Store::open`은 스키마를 올리고 권한을 좁힌다. 진단(`doctor`)과 접속 키의 세대 확인은 그러면
//! 안 되므로 있는 파일만 읽기 전용으로 연다. WAL 저장소는 읽기 전용으로 열어도 곁 파일(`-wal` · `-shm`)이
//! 생긴다. 그래서 둘 다 없을 때(쓰는 프로그램이 없고 깨끗이 닫힌 저장소)는 `immutable=1`로 열어 아무 파일도
//! 만들지 않는다. 곁 파일이 있으면(도는 서버가 있다) 보통의 읽기 전용으로 연다.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

/// 저장소의 잠금을 기다리는 상한(도는 서버가 쓰는 중이어도 오래 기다리지 않는다).
const BUSY_WAIT: Duration = Duration::from_millis(500);

/// 경로를 SQLite URI(`file:…`)의 길 부분으로 옮긴다. `%` · `?` · `#`만 바꾼다(SQLite 문서의 규칙).
fn uri_path(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    // 드라이브 글자로 시작하면(Windows) 앞에 `/`를 붙인다.
    if text.as_bytes().get(1) == Some(&b':') {
        text.insert(0, '/');
    }
    text.replace('%', "%25")
        .replace('?', "%3f")
        .replace('#', "%23")
}

/// 곁 파일(`<저장소>-wal` · `<저장소>-shm`)이 하나라도 있나.
pub(crate) fn has_side_files(path: &Path) -> bool {
    ["-wal", "-shm"].iter().any(|suffix| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        Path::new(&name).exists()
    })
}

/// 있는 저장소 파일을 읽기 전용으로 연다. 새로 만들거나 이전하거나 권한을 바꾸지 않는다.
/// 곁 파일이 둘 다 없으면 `immutable=1`로 열어 곁 파일도 만들지 않는다.
pub(crate) fn open_read_only(path: &Path) -> Result<Connection, rusqlite::Error> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = if has_side_files(path) {
        Connection::open_with_flags(path, flags)?
    } else {
        let uri = format!("file:{}?immutable=1", uri_path(path));
        Connection::open_with_flags(uri, flags | OpenFlags::SQLITE_OPEN_URI)?
    };
    let _ = conn.busy_timeout(BUSY_WAIT);
    Ok(conn)
}

/// 잠금에 걸렸나(다른 프로그램이 쓰는 중).
pub(crate) fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

/// 지금 접속 키의 세대. 키를 둔 적이 없으면 `Ok(None)`.
pub(crate) fn access_key_generation(conn: &Connection) -> Result<Option<u64>, rusqlite::Error> {
    let found: Option<i64> =
        conn.query_row("SELECT MAX(generation) FROM key_hashes", [], |row| {
            row.get(0)
        })?;
    Ok(found.and_then(|generation| u64::try_from(generation).ok()))
}

/// 보관 폴더의 저장소에서 접속 키의 세대를 읽기만 한다. 저장소가 없거나 읽지 못하면 `None`,
/// 읽었는데 키가 없으면 `Some(None)`.
pub(crate) fn stored_generation(file: &Path) -> Option<Option<u64>> {
    if !file.is_file() {
        return None;
    }
    let conn = open_read_only(file).ok()?;
    access_key_generation(&conn).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_는_물음표와_샵을_바꾼다() {
        assert_eq!(uri_path(Path::new("/a b/c?d#e%f")), "/a b/c%3fd%23e%25f");
        assert_eq!(uri_path(Path::new("C:\\pi\\db")), "/C:/pi/db");
    }

    #[test]
    fn 읽기만_하고_곁_파일을_만들지_않는다() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.db");
        {
            let conn = Connection::open(&file).unwrap();
            conn.pragma_update(None, "journal_mode", "WAL").unwrap();
            conn.execute_batch(
                "CREATE TABLE key_hashes (generation INTEGER); INSERT INTO key_hashes VALUES (3);",
            )
            .unwrap();
        }
        assert!(!has_side_files(&file));
        assert_eq!(stored_generation(&file), Some(Some(3)));
        assert!(!has_side_files(&file), "곁 파일이 생기지 않는다");
        assert_eq!(stored_generation(&dir.path().join("none.db")), None);
    }
}
