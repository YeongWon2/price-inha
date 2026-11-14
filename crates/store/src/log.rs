//! 쌓기만 하는 기록: 이벤트(C2 · E3)와 접속 기록(M6). 번호는 표마다 따로 1부터 센다.

use rusqlite::{Connection, params};

use crate::codec::{self, insert_returning_seq};
use crate::error::StoreError;
use crate::model::{AccessRecord, Event, NewAccess, UnixMillis};

/// 이벤트를 쌓고 번호를 준다.
pub(crate) fn append(
    conn: &Connection,
    kind: &str,
    payload: &serde_json::Value,
) -> Result<u64, StoreError> {
    let text = serde_json::to_string(payload)?;
    insert_returning_seq(
        conn,
        "INSERT INTO events (seq, kind, payload)
         SELECT COALESCE(MAX(seq), 0) + 1, ?1, ?2 FROM events
         RETURNING seq",
        params![kind, text],
    )
}

/// 번호가 `after`보다 큰 이벤트를 번호 순서로 `max`개까지.
pub(crate) fn read_after(
    conn: &Connection,
    after: u64,
    max: usize,
) -> Result<Vec<Event>, StoreError> {
    // 번호는 i64 범위 안에서만 생긴다. 그보다 큰 `after` 뒤에는 아무것도 없다.
    let Ok(after) = i64::try_from(after) else {
        return Ok(Vec::new());
    };
    let limit = i64::try_from(max).unwrap_or(i64::MAX);
    let mut stmt =
        conn.prepare("SELECT seq, kind, payload FROM events WHERE seq > ?1 ORDER BY seq LIMIT ?2")?;
    let mut rows = stmt.query(params![after, limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let text: String = row.get(2)?;
        out.push(Event {
            seq: codec::seq(row, 0)?,
            kind: row.get(1)?,
            payload: serde_json::from_str(&text)?,
        });
    }
    Ok(out)
}

/// 접속 기록 한 줄을 쌓고 번호를 준다.
pub(crate) fn record_access(conn: &Connection, access: NewAccess) -> Result<u64, StoreError> {
    insert_returning_seq(
        conn,
        "INSERT INTO access_log (seq, at, origin, action)
         SELECT COALESCE(MAX(seq), 0) + 1, ?1, ?2, ?3 FROM access_log
         RETURNING seq",
        params![access.at.0, access.origin, access.action],
    )
}

/// 접속 기록 전체, 번호 순서.
pub(crate) fn access_log(conn: &Connection) -> Result<Vec<AccessRecord>, StoreError> {
    let mut stmt = conn.prepare("SELECT seq, at, origin, action FROM access_log ORDER BY seq")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(AccessRecord {
            seq: codec::seq(row, 0)?,
            at: UnixMillis(row.get(1)?),
            origin: row.get(2)?,
            action: row.get(3)?,
        });
    }
    Ok(out)
}
