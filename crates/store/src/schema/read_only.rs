//! (23차) 읽기 전용 열기: 있는 파일만, 판을 올리지 않고, 곁 파일 · 권한을 건드리지 않는다.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use super::{APPLICATION_ID, user_version};
use crate::error::StoreError;

/// 읽기 전용 손잡이가 잠금을 기다리는 상한. 도는 서버가 잠깐 쥐고 있어도 오래 기다리지 않는다
/// (WAL에서는 읽기가 쓰기를 막지도 막히지도 않는다 — 곁 파일을 처음 맞출 때만 잠깐 기다릴 수 있다).
const READ_BUSY_WAIT: Duration = Duration::from_millis(500);

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

/// 곁 파일(`<저장소>-wal` · `<저장소>-shm`)이 하나라도 있나(쓰는 프로그램이 있거나 깨끗이 닫히지 않았다).
fn has_side_files(path: &Path) -> bool {
    ["-wal", "-shm"].iter().any(|suffix| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        Path::new(&name).exists()
    })
}

/// 있는 저장소 파일을 읽기 전용으로 열고 지금 판을 준다.
///
/// - 없으면(파일이 아니면) [`StoreError::FileAccess`]이고 아무것도 만들지 않는다.
/// - 곁 파일이 둘 다 없으면 `immutable=1`로 열어 곁 파일도 만들지 않는다. 있으면(도는 서버) 보통의 읽기 전용으로
///   열어 WAL의 쓴 것까지 본다. 어느 쪽도 쓰기 잠금을 잡지 않는다.
/// - 이 저장소의 파일이 아니면 [`StoreError::Incompatible`]. 아는 판보다 새 파일도 연다(읽기만 하므로 —
///   부르는 쪽이 판을 보고 정한다).
pub(crate) fn open_read_only(path: &Path) -> Result<(Connection, u32), StoreError> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Err(StoreError::FileAccess {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "파일이 아니다"),
            });
        }
        Err(source) => {
            return Err(StoreError::FileAccess {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = if has_side_files(path) {
        Connection::open_with_flags(path, flags)?
    } else {
        let uri = format!("file:{}?immutable=1", uri_path(path));
        Connection::open_with_flags(uri, flags | OpenFlags::SQLITE_OPEN_URI)?
    };
    conn.busy_timeout(READ_BUSY_WAIT)?;
    let version = user_version(&conn)?;
    if version == 0 {
        let objects: i64 =
            conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))?;
        if objects > 0 {
            return Err(StoreError::Incompatible(
                "이 저장소가 만든 파일이 아니다(다른 표가 이미 있다)".to_string(),
            ));
        }
        return Ok((conn, version));
    }
    let app_id: i64 = conn.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if app_id != APPLICATION_ID {
        return Err(StoreError::Incompatible(
            "이 저장소가 만든 파일이 아니다(표시가 다르다)".to_string(),
        ));
    }
    Ok((conn, version))
}

/// `PRAGMA integrity_check`가 ok인가.
pub(crate) fn integrity_ok(conn: &Connection) -> Result<bool, StoreError> {
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    Ok(result == "ok")
}
