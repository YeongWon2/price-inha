//! 쌓기만 하는 기록: 이벤트(C2 · E3)와 접속 기록(M6). 번호는 표마다 따로 1부터 센다.

use rusqlite::{Connection, params};

use crate::codec::{self, insert_returning_seq};
use crate::error::StoreError;
use crate::model::{AccessRecord, Event, EventRange, NewAccess, TimedEvent, UnixMillis};
use crate::{EVENT_KIND_MAX_CHARS, REJECTED_KIND_KEEP_CHARS};

/// 이벤트를 쌓고 번호를 준다. 시각은 있으면 칸에 둔다(내용에 싸 넣지 않는다).
///
/// 종류 이름이 규칙에 어긋나면 아무것도 쓰지 않고 거부한다(번호를 쓰지 않는다).
pub(crate) fn append(
    conn: &Connection,
    kind: &str,
    payload: &serde_json::Value,
    at: Option<UnixMillis>,
) -> Result<u64, StoreError> {
    let kind = checked_kind(kind)?;
    let text = serde_json::to_string(payload)?;
    insert_returning_seq(
        conn,
        "INSERT INTO events (seq, kind, payload, at)
         SELECT COALESCE(MAX(seq), 0) + 1, ?1, ?2, ?3 FROM events
         RETURNING seq",
        params![kind, text, at.map(|at| at.0)],
    )
}

/// 이벤트 종류 이름을 검사하고 저장할 이름을 준다(3차, 대조표 C9).
///
/// 원본(Hermes `gateway/hosted_rooms_common.py`의 이름 검사)과 같게: 앞뒤 공백을 떼어 낸 뒤
/// 비어 있지 않고, [`EVENT_KIND_MAX_CHARS`]자 이하이고, 전체가 `^[a-z][a-z0-9_.-]*$`여야 한다.
/// 공백은 원본(Python `str.strip`)이 떼는 것과 같게 본다. 거부할 때는 받은 이름 그대로를 담되,
/// [`REJECTED_KIND_KEEP_CHARS`]자보다 길면 잘라 담는다([`rejected_name`]).
fn checked_kind(kind: &str) -> Result<&str, StoreError> {
    let trimmed = kind.trim_matches(is_strip_space);
    let mut chars = trimmed.chars();
    let shaped = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | '-'));
    if shaped && trimmed.chars().count() <= EVENT_KIND_MAX_CHARS {
        Ok(trimmed)
    } else {
        Err(StoreError::InvalidEventKind {
            kind: rejected_name(kind),
        })
    }
}

/// 오류에 담을 거부한 이름(4차): 글자 단위로 앞 [`REJECTED_KIND_KEEP_CHARS`]자까지, 더 길면 뒤에 `…`.
fn rejected_name(kind: &str) -> String {
    match kind.char_indices().nth(REJECTED_KIND_KEEP_CHARS) {
        Some((cut, _)) => format!("{}…", &kind[..cut]),
        None => kind.to_string(),
    }
}

/// Python `str.isspace`와 같은 공백: 유니코드 공백에 정보 구분 문자(U+001C ~ U+001F)를 더한 것.
fn is_strip_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
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

/// 범위에 드는 이벤트를 시각과 함께, 번호 순서로 `max`개까지.
///
/// 시각 조건은 경계를 빼고(같은 시각은 빠진다), 시각 칸이 빈 줄은 시각 조건이 있으면 빠진다
/// (SQL에서 빈 칸과의 비교는 참이 아니다). 시각 칸이 정수가 아닌 줄(7차)은 시각 없음으로 읽고,
/// 시각 조건이 있으면 빠진다.
pub(crate) fn read_events(
    conn: &Connection,
    range: EventRange,
    max: usize,
) -> Result<Vec<TimedEvent>, StoreError> {
    // 번호는 i64 범위 안에서만 생긴다. 그보다 큰 번호 뒤에는 아무것도 없다.
    let Ok(after) = i64::try_from(range.after_seq) else {
        return Ok(Vec::new());
    };
    let limit = i64::try_from(max).unwrap_or(i64::MAX);
    let after_at = range.after_at.map(|at| at.0);
    let before_at = range.before_at.map(|at| at.0);
    let mut stmt = conn.prepare(&events_query(after_at.is_some(), before_at.is_some()))?;
    let mut binds: Vec<i64> = vec![after];
    binds.extend(after_at);
    binds.extend(before_at);
    binds.push(limit);
    let mut rows = stmt.query(rusqlite::params_from_iter(binds))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        // 시각 칸이 정수가 아니면(버전 2 파일에서 i64 범위 밖의 `at`이 실수로 옮겨 적힌 줄, 7차)
        // 시각 없음으로 읽는다.
        let at = match row.get_ref(2)? {
            rusqlite::types::ValueRef::Integer(at) => Some(UnixMillis(at)),
            _ => None,
        };
        let text: String = row.get(3)?;
        out.push(TimedEvent {
            seq: codec::seq(row, 0)?,
            kind: row.get(1)?,
            at,
            payload: serde_json::from_str(&text)?,
        });
    }
    Ok(out)
}

/// [`read_events`]의 질의. 시각 조건은 주어진 것만 넣는다(4차): `?2 IS NULL OR at > ?2` 모양은
/// SQLite가 `events(at)` 색인을 쓰지 못하므로 조건마다 질의를 따로 만든다.
/// 시각 조건이 있으면 `INDEXED BY events_by_at`로 시각 색인을 쓰게 못 박는다: 시각 한 끝만 있고
/// `ORDER BY seq LIMIT`이 붙으면 SQLite는 통계 없이 번호 순서로 표를 훑는 쪽을 고르기 때문이다
/// (그러면 30만 줄을 다 훑을 수 있다). 색인을 쓸 수 없게 되면 질의가 조용히 훑지 않고 오류를 낸다.
/// 자리 표시는 차례대로 번호 · (아래 끝 시각) · (위 끝 시각) · 개수다.
fn events_query(after_at: bool, before_at: bool) -> String {
    let source = if after_at || before_at {
        "events INDEXED BY events_by_at"
    } else {
        "events"
    };
    let mut sql = format!("SELECT seq, kind, at, payload FROM {source} WHERE seq > ?");
    if after_at || before_at {
        // 시각 칸에 실수가 적힌 줄은 시각 없음이므로 시각 조건에서 빠진다(7차).
        sql.push_str(" AND typeof(at) = 'integer'");
    }
    if after_at {
        sql.push_str(" AND at > ?");
    }
    if before_at {
        sql.push_str(" AND at < ?");
    }
    sql.push_str(" ORDER BY seq LIMIT ?");
    sql
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 이름은_원본처럼_앞뒤_공백을_떼어_내고_검사한다() {
        assert_eq!(checked_kind(" chat\n").unwrap(), "chat");
        assert_eq!(
            checked_kind("\u{1f}run.started\u{3000}").unwrap(),
            "run.started"
        );
        // 떼어 낸 뒤 64자면 받는다.
        let padded = format!("  {}  ", "a".repeat(EVENT_KIND_MAX_CHARS));
        assert!(checked_kind(&padded).is_ok());
        for bad in ["", "  ", " A ", " a b ", "\u{200b}a"] {
            match checked_kind(bad) {
                Err(StoreError::InvalidEventKind { kind }) => assert_eq!(kind, bad),
                other => panic!("{bad:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn 거부한_이름은_글자_단위로_잘라_담는다() {
        let keep = "가".repeat(REJECTED_KIND_KEEP_CHARS);
        assert_eq!(rejected_name(&keep), keep);
        assert_eq!(rejected_name(&format!("{keep}나")), format!("{keep}…"));
        assert_eq!(rejected_name(""), "");
    }

    /// 실제 `read_events` 질의의 계획 줄들.
    fn plan_of(conn: &Connection, after_at: bool, before_at: bool) -> Vec<String> {
        let sql = format!("EXPLAIN QUERY PLAN {}", events_query(after_at, before_at));
        let mut binds = vec![0_i64];
        binds.extend(after_at.then_some(1));
        binds.extend(before_at.then_some(5));
        binds.push(100);
        conn.prepare(&sql)
            .unwrap()
            .query_map(rusqlite::params_from_iter(binds), |row| row.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn 시각_조건이_있는_실제_질의는_시각_색인을_쓰고_표를_훑지_않는다() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi.sqlite");
        let store = crate::Store::open(&path).unwrap();
        for n in 0..50 {
            store
                .append_at("chat", &serde_json::json!(n), UnixMillis(n))
                .unwrap();
        }
        let conn = store.lock();
        for (after_at, before_at) in [(true, true), (true, false), (false, true)] {
            let lines = plan_of(&conn, after_at, before_at);
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with("SEARCH events USING INDEX events_by_at")),
                "{after_at} {before_at}: {lines:?}"
            );
            assert!(
                !lines.iter().any(|l| l.starts_with("SCAN events")),
                "{lines:?}"
            );
        }
        // 시각 조건이 없으면 번호로 찾는다.
        let lines = plan_of(&conn, false, false);
        assert!(
            lines.iter().any(|l| l.contains("INTEGER PRIMARY KEY")),
            "{lines:?}"
        );
    }

    #[test]
    fn 떼어_낸_이름으로_저장된다() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path().join("pi.sqlite")).unwrap();
        assert_eq!(store.append("  chat ", &serde_json::json!(1)).unwrap(), 1);
        assert_eq!(store.read_after(0, 10).unwrap()[0].kind, "chat");
    }
}
