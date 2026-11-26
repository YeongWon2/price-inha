//! (21차, 결정 31) 저장소 쓰임 재기와 공간 돌려받기.
//!
//! - 재기: 표마다(이벤트 · 방 로그는 종류마다) 줄 수 · 바이트 · 처음과 마지막 시각. 바이트는 칸 길이의 합으로
//!   어림한다(묶음 SQLite에 `dbstat`이 없다). 파일 크기는 `page_count × page_size`.
//! - 공간 돌려받기: 덜어 낸 뒤 빈 쪽을 파일에서 돌려준다(incremental vacuum). 자동 정리 방식이 incremental이
//!   아닌 파일(판 12 전에 만든 파일)은 처음 한 번 방식을 바꾸고 전체 정리(VACUUM)를 한다.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::{Store, StoreError};

/// 한 묶음(표, 또는 표 안의 종류)의 쓰임.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageGroup {
    /// 표 이름.
    pub table: String,
    /// 종류(이벤트 · 방 로그만). 나머지 표는 `None`.
    pub kind: Option<String>,
    /// 줄 수.
    pub rows: u64,
    /// 칸 길이의 합(바이트, 어림).
    pub bytes: u64,
    /// 가장 이른 시각(시각 칸 `at`이 있는 표만).
    pub first_at: Option<i64>,
    /// 가장 늦은 시각.
    pub last_at: Option<i64>,
}

/// 저장소 쓰임.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageUsage {
    /// 쪽 크기.
    pub page_size: u64,
    /// 쪽 수.
    pub page_count: u64,
    /// 빈 쪽 수.
    pub freelist_count: u64,
    /// 묶음들(바이트 큰 것부터).
    pub groups: Vec<UsageGroup>,
}

/// 공간 돌려받기 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclaimReport {
    /// 돌려받기 전 빈 쪽 수.
    pub free_before: u64,
    /// 돌려받은 뒤 빈 쪽 수.
    pub free_after: u64,
    /// 이번에 자동 정리 방식을 바꾸느라 한 번 전체 정리(VACUUM)를 했는가.
    pub converted: bool,
}

impl StorageUsage {
    /// 파일 크기(`page_count × page_size`).
    pub fn file_bytes(&self) -> u64 {
        self.page_count.saturating_mul(self.page_size)
    }
}

/// 저장소 파일을 읽기 전용으로 열어 잰다(판을 올리지 않고 아무것도 바꾸지 않는다). 파일이 없으면 `None`.
pub fn measure_file(path: &Path) -> Result<Option<StorageUsage>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    // 곁 파일(-wal · -shm)이 없으면 immutable로 열어 곁 파일도 만들지 않는다(닫힌 저장소를 그대로 둔다).
    let side = ["-wal", "-shm"].iter().any(|suffix| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        Path::new(&name).exists()
    });
    let conn = if side {
        Connection::open_with_flags(path, flags)?
    } else {
        let text = path
            .to_string_lossy()
            .replace('\\', "/")
            .replace('%', "%25")
            .replace('?', "%3f")
            .replace('#', "%23");
        let text = if text.as_bytes().get(1) == Some(&b':') {
            format!("/{text}")
        } else {
            text
        };
        Connection::open_with_flags(
            format!("file:{text}?immutable=1"),
            flags | OpenFlags::SQLITE_OPEN_URI,
        )?
    };
    let tx = conn.unchecked_transaction()?;
    measure(&tx).map(Some)
}

impl Store {
    /// 열린 저장소의 쓰임.
    pub fn storage_usage(&self) -> Result<StorageUsage, StoreError> {
        self.read(measure)
    }

    /// 빈 쪽을 많아야 `max_pages`개 파일에서 돌려준다. 자동 정리 방식이 incremental이 아니면 한 번 바꾼다
    /// (그때는 전체 정리라 빈 쪽이 모두 돌아간다). 묶음 밖에서 돌고, 도는 동안 이 손잡이의 다른 일은 기다린다.
    pub fn reclaim_space(&self, max_pages: u32) -> Result<ReclaimReport, StoreError> {
        let conn = self.lock();
        let free_before = pragma_u64(&conn, "freelist_count")?;
        let mode = pragma_u64(&conn, "auto_vacuum")?;
        let converted = mode != 2;
        if converted {
            conn.pragma_update(None, "auto_vacuum", 2)?;
            conn.execute_batch("VACUUM")?;
        } else {
            conn.execute_batch(&format!("PRAGMA incremental_vacuum({max_pages})"))?;
        }
        let free_after = pragma_u64(&conn, "freelist_count")?;
        Ok(ReclaimReport {
            free_before,
            free_after,
            converted,
        })
    }
}

fn pragma_u64(conn: &Connection, name: &str) -> Result<u64, StoreError> {
    let value: i64 = conn.pragma_query_value(None, name, |row| row.get(0))?;
    Ok(u64::try_from(value).unwrap_or(0))
}

fn measure(conn: &Connection) -> Result<StorageUsage, StoreError> {
    let mut groups = Vec::new();
    for table in tables(conn)? {
        match table.as_str() {
            "events" => groups.extend(by_kind(
                conn,
                "events",
                "length(kind) + length(payload) + 16",
            )?),
            "room_events" => groups.extend(by_kind(
                conn,
                "room_events",
                "length(room) + length(event_id) + length(kind) + length(actor_kind) \
                 + length(actor_id) + length(payload) + 24",
            )?),
            _ => groups.push(whole_table(conn, &table)?),
        }
    }
    groups.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.table.cmp(&b.table))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    Ok(StorageUsage {
        page_size: pragma_u64(conn, "page_size")?,
        page_count: pragma_u64(conn, "page_count")?,
        freelist_count: pragma_u64(conn, "freelist_count")?,
        groups,
    })
}

/// 표 이름들(가상 표 · SQLite 내부 표 빼고). 가상 표의 그림자 표는 실제 저장이라 든다.
fn tables(conn: &Connection) -> Result<Vec<String>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
           AND sql NOT LIKE 'CREATE VIRTUAL%' ORDER BY name",
    )?;
    let names = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(names)
}

fn count_of(value: Option<i64>) -> u64 {
    value.and_then(|v| u64::try_from(v).ok()).unwrap_or(0)
}

fn by_kind(conn: &Connection, table: &str, size: &str) -> Result<Vec<UsageGroup>, StoreError> {
    let sql =
        format!("SELECT kind, COUNT(*), SUM({size}), MIN(at), MAX(at) FROM {table} GROUP BY kind");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageGroup {
            table: table.to_owned(),
            kind: Some(row.get(0)?),
            rows: count_of(row.get(1)?),
            bytes: count_of(row.get(2)?),
            first_at: row.get(3)?,
            last_at: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn whole_table(conn: &Connection, table: &str) -> Result<UsageGroup, StoreError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<String>, _>>()?;
    let mut size: Vec<String> = columns
        .iter()
        .map(|c| format!("COALESCE(length(\"{c}\"), 0)"))
        .collect();
    size.push("8".to_owned());
    let time = if columns.iter().any(|c| c == "at") {
        "MIN(at), MAX(at)"
    } else {
        "NULL, NULL"
    };
    let sql = format!(
        "SELECT COUNT(*), SUM({}), {time} FROM \"{table}\"",
        size.join(" + ")
    );
    let group = conn.query_row(&sql, [], |row| {
        Ok(UsageGroup {
            table: table.to_owned(),
            kind: None,
            rows: count_of(row.get(0)?),
            bytes: count_of(row.get(1)?),
            first_at: row.get(2)?,
            last_at: row.get(3)?,
        })
    })?;
    Ok(group)
}
