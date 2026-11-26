//! (21차, 결정 31) 보관 묶음의 SQL: 덜어 낼 줄 고르기 · JSON 줄 만들기 · 묶음 남기고 지우기.

use rusqlite::{Connection, Row, params};
use serde_json::{Value, json};

use crate::StoreError;
use crate::archive::{ArchiveBatch, ArchiveCategory, ArchiveExport, NewArchiveBatch, fingerprint};
use crate::model::UnixMillis;

/// 보관 묶음 표의 칸(읽는 순서).
pub(crate) const BATCH_COLUMNS: &str = "id, category, period, from_at, to_at, upto_rowid, \
     row_count, fingerprint, file, file_sha256, summary, created_at";

/// 보관 묶음 한 줄. 모르는 묶음 글자는 [`StoreError::Incompatible`].
pub(crate) fn batch_from(row: &Row<'_>) -> rusqlite::Result<Result<ArchiveBatch, StoreError>> {
    let code: String = row.get(1)?;
    let Some(category) = ArchiveCategory::from_code(&code) else {
        return Ok(Err(StoreError::Incompatible(format!(
            "보관 묶음 글자가 이상하다: {code}"
        ))));
    };
    let row_count: i64 = row.get(6)?;
    Ok(Ok(ArchiveBatch {
        id: row.get(0)?,
        batch: NewArchiveBatch {
            category,
            period: row.get(2)?,
            from_at: UnixMillis(row.get(3)?),
            to_at: UnixMillis(row.get(4)?),
            upto_rowid: row.get(5)?,
            row_count: u64::try_from(row_count).unwrap_or(0),
            fingerprint: row.get(7)?,
            file: row.get(8)?,
            file_sha256: row.get(9)?,
            summary: row.get(10)?,
            created_at: UnixMillis(row.get(11)?),
        },
    }))
}

/// 덜어 낼 수 있는 줄의 조건(종류 · 시각 있음 · 표의 마지막 줄이 아님). 지우기 트리거(판 12)와 같은 조건이다.
pub(crate) fn candidate_filter(category: ArchiveCategory) -> String {
    let kinds: Vec<String> = category.kinds().iter().map(|k| format!("'{k}'")).collect();
    let last = if category.table() == "room_events" {
        "seq < (SELECT MAX(r.seq) FROM room_events r WHERE r.room = room_events.room)"
    } else {
        "seq < (SELECT MAX(e.seq) FROM events e)"
    };
    format!(
        "kind IN ({}) AND at IS NOT NULL AND {last}",
        kinds.join(", ")
    )
}

/// 내용 칸: JSON이면 값으로, 아니면 글자로 싣는다.
fn payload_value(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

fn room_line(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "room": row.get::<_, String>(1)?,
        "seq": row.get::<_, i64>(2)?,
        "event_id": row.get::<_, String>(3)?,
        "kind": row.get::<_, String>(4)?,
        "actor_kind": row.get::<_, String>(5)?,
        "actor_id": row.get::<_, String>(6)?,
        "at": row.get::<_, i64>(7)?,
        "payload": payload_value(row.get(8)?),
    }))
}

fn event_line(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "seq": row.get::<_, i64>(1)?,
        "kind": row.get::<_, String>(2)?,
        "at": row.get::<_, i64>(3)?,
        "payload": payload_value(row.get(4)?),
    }))
}

/// 고른 줄들(rowid 순서)을 JSON 줄로 낸다. `upto`가 있으면 그 rowid 이하만.
pub(crate) fn export(
    conn: &Connection,
    category: ArchiveCategory,
    (from_at, to_at): (UnixMillis, UnixMillis),
    upto: Option<i64>,
) -> Result<ArchiveExport, StoreError> {
    let room = category.table() == "room_events";
    let columns = if room {
        "rowid, room, seq, event_id, kind, actor_kind, actor_id, at, payload"
    } else {
        "rowid, seq, kind, at, payload"
    };
    let sql = format!(
        "SELECT {columns} FROM {} WHERE {} AND at >= ?1 AND at < ?2 AND rowid <= ?3 \
         ORDER BY rowid",
        category.table(),
        candidate_filter(category)
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![from_at.0, to_at.0, upto.unwrap_or(i64::MAX)])?;
    let mut lines = Vec::new();
    let mut upto_rowid = 0;
    while let Some(row) = rows.next()? {
        upto_rowid = row.get(0)?;
        let line = if room {
            room_line(row)?
        } else {
            event_line(row)?
        };
        lines.push(line.to_string());
    }
    let fingerprint = fingerprint(&lines);
    Ok(ArchiveExport {
        category,
        from_at,
        to_at,
        upto_rowid,
        lines,
        fingerprint,
    })
}

/// 같은 묶음 안에서 다시 골라 견주고, 묶음 줄을 남기고, 그 줄들을 지운다.
pub(crate) fn commit(
    conn: &Connection,
    batch: NewArchiveBatch,
) -> Result<ArchiveBatch, StoreError> {
    let range = (batch.from_at, batch.to_at);
    let again = export(conn, batch.category, range, Some(batch.upto_rowid))?;
    if again.lines.len() as u64 != batch.row_count || again.fingerprint != batch.fingerprint {
        return Err(StoreError::ArchiveMismatch(format!(
            "다시 고른 줄 {}개 · 지문 {}가 받은 {}개 · {}와 다르다",
            again.lines.len(),
            again.fingerprint,
            batch.row_count,
            batch.fingerprint
        )));
    }
    let row_count = i64::try_from(batch.row_count)
        .map_err(|_| StoreError::ArchiveMismatch("줄 수가 너무 크다".to_owned()))?;
    conn.execute(
        "INSERT INTO archive_batches (category, period, from_at, to_at, upto_rowid, row_count,
             fingerprint, file, file_sha256, summary, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            batch.category.code(),
            batch.period,
            batch.from_at.0,
            batch.to_at.0,
            batch.upto_rowid,
            row_count,
            batch.fingerprint,
            batch.file,
            batch.file_sha256,
            batch.summary,
            batch.created_at.0
        ],
    )?;
    let id = conn.last_insert_rowid();
    let deleted = conn.execute(
        &format!(
            "DELETE FROM {} WHERE {} AND at >= ?1 AND at < ?2 AND rowid <= ?3",
            batch.category.table(),
            candidate_filter(batch.category)
        ),
        params![batch.from_at.0, batch.to_at.0, batch.upto_rowid],
    )?;
    if deleted as u64 != batch.row_count {
        return Err(StoreError::ArchiveMismatch(format!(
            "지운 줄 {deleted}개가 {}개와 다르다",
            batch.row_count
        )));
    }
    Ok(ArchiveBatch { id, batch })
}
