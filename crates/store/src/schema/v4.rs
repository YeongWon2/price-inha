//! 스키마 버전 3 → 4(4차 작업): 이벤트 시각 색인과 옛 기록 형식의 경계 번호.
//!
//! - `events(at)` 색인. 시각 범위 읽기([`crate::Store::read_events`])가 표 전체를 훑지 않는다.
//! - 옛 기록 형식의 경계 번호를 표 `legacy_boundary`의 한 줄(`id = 1`)에 둔다. 경계가 없으면
//!   그 줄의 `until_seq`가 비어 있다(줄은 언제나 하나 있다). 줄을 넣은 뒤 넣기 · 고치기 · 지우기를
//!   모두 막는 트리거를 걸어, 기록 표처럼 SQL로도 바뀌지 않는다(`INSERT OR REPLACE` ·
//!   `ON CONFLICT DO UPDATE`는 넣기 트리거가, `UPDATE OR …`는 고치기 트리거가 먼저 막는다).
//!
//! 경계는 이전을 시작한 버전에 따라 다르게 정하므로, 값은 버전 3 단계를 돌리기 **전에** 읽어
//! 두고([`boundary_before`]) 모든 단계를 돌린 뒤 넣는다([`seal`]). 둘 다 [`super::migrate`]의 한
//! 트랜잭션 안이라 중간에 실패하면 파일은 이전 전 그대로이고, 고친 뒤 다시 열면 같은 값을 정한다.
//! 이미 나간 버전 1 ~ 3의 SQL은 고치지 않는다.

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension};

use crate::error::StoreError;

/// 경계 번호를 정하는 스키마 버전.
pub(super) const VERSION: u32 = 4;

/// 색인과 경계 표. 경계 줄은 [`seal`]이 넣는다.
pub(super) const V4: &str = "
CREATE INDEX events_by_at ON events (at);
CREATE TABLE legacy_boundary (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    until_seq INTEGER
);
CREATE TRIGGER legacy_boundary_no_update BEFORE UPDATE ON legacy_boundary
BEGIN SELECT RAISE(ABORT, 'legacy_boundary is fixed'); END;
CREATE TRIGGER legacy_boundary_no_delete BEFORE DELETE ON legacy_boundary
BEGIN SELECT RAISE(ABORT, 'legacy_boundary is fixed'); END;
";

/// 경계 줄을 넣은 뒤 거는, 더 넣기를 막는 트리거.
const SEAL: &str = "
CREATE TRIGGER legacy_boundary_no_insert BEFORE INSERT ON legacy_boundary
BEGIN SELECT RAISE(ABORT, 'legacy_boundary is fixed'); END;
";

/// 옛 봉투 줄을 가르는 SQL 조건(`events` 표의 `payload` · `at` 칸): 내용이 **정확히 두 칸**
/// (`at` · `payload`)인 객체이고, `at`이 정수이고, 시각 칸이 비었거나 내용의 `at`과 같다.
/// 두 칸은 `json_each`로 센다. `CASE`는 JSON이 아닌 내용에서 JSON 함수가 오류를 내지 않게 먼저
/// `json_valid`를 보려는 것이다. 이 조건을 지난 줄도 [`readable_by_gate`]를 지나야 옛 봉투 줄이다.
/// 경계를 정할 때([`boundary_before`])와 옛 봉투 번호를 줄 때([`legacy_envelope_seqs`])가 이 하나를
/// 함께 쓴다(7차: 규칙을 두 벌로 두지 않는다).
///
/// 5차에서 좁힘(2026-11-15): `json_type`은 JSON 글자의 모양만 보아 i64 범위 밖의 정수와 `-0`도
/// `'integer'`라 한다. 그래서 꺼낸 값이 SQL 정수인지(`typeof(json_extract(…))`: 범위 밖이면 실수로
/// 꺼낸다)와, 적힌 글자(`->`가 원래 글자를 돌려준다)가 `-0`이 아닌지를 더 본다. 맨 위 칸의 이름이
/// 겹친 객체는 `json_each`가 겹친 칸도 하나씩 세므로 두 칸 조건에서 이미 빠진다.
/// 시각 칸이 정수가 아닌 줄(버전 2에서 범위 밖 `at`이 실수로 옮겨 적힌 줄)은 내용의 `at`도 범위
/// 밖이라 정수 조건에서 이미 빠진다.
///
/// 6차에서 좁힘(2026-11-15): SQLite `json_valid`는 짝 없는 서로게이트 이스케이프(`"\ud800"`)가
/// 든 글자도 JSON으로 받지만 `serde_json`(접속 관문)은 거부한다. 그래서 그 판정은
/// [`readable_by_gate`]가 원래 글자로 한다(관문과 같게).
/// (바로잡음 2026-11-14: 처음 SQL은 시각 칸이 채워진 줄만 보고 칸 수를 세지 않았다. 버전 4는 이번
/// 차수 안에서 아직 어느 실제 파일에도 쓰이지 않았으므로 버전 5를 따로 두지 않고 이 단계를 고친다.
/// 경계는 버전 3 → 4로 올리는 순간에만 정하고, 이미 버전 4인 파일의 경계는 바뀌지 않는다.)
macro_rules! envelope_condition {
    () => {
        "CASE WHEN json_valid(payload) THEN
      json_type(payload) = 'object'
      AND (SELECT COUNT(*) FROM json_each(payload)) = 2
      AND json_type(payload, '$.at') = 'integer'
      AND typeof(json_extract(payload, '$.at')) = 'integer'
      AND (payload -> '$.at') <> '-0'
      AND json_type(payload, '$.payload') IS NOT NULL
      AND (at IS NULL OR json_extract(payload, '$.at') = at)
  ELSE 0 END"
    };
}

/// 이미 버전 3인 파일의 옛 봉투 후보 줄, 번호가 큰 순서.
const V3_FILLED_ENVELOPES: &str = concat!(
    "SELECT seq, payload FROM events WHERE ",
    envelope_condition!(),
    " ORDER BY seq DESC"
);

/// 번호가 `?1`보다 크고 `?2` 이하인 옛 봉투 후보 줄, 번호 순서(7차).
const ENVELOPES_BETWEEN: &str = concat!(
    "SELECT seq, payload FROM events WHERE seq > ?1 AND seq <= ?2 AND ",
    envelope_condition!(),
    " ORDER BY seq"
);

/// [`envelope_condition`]을 지난 후보 줄이 접속 관문(`serde_json`)에도 읽히는가. 짝 없는 서로게이트
/// 이스케이프가 든 글자 등은 관문이 옛 봉투로 풀 수 없으므로 옛 봉투 줄이 아니다.
fn readable_by_gate(payload: ValueRef<'_>) -> bool {
    match payload {
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .is_some_and(|text| serde_json::from_str::<serde_json::Value>(text).is_ok()),
        _ => false,
    }
}

/// 후보 질의를 돌려 옛 봉투 줄의 번호를 질의 순서대로 `visit`에 넘긴다. `visit`이 거짓을 주면 멈춘다.
fn scan_envelopes(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    mut visit: impl FnMut(i64) -> bool,
) -> Result<(), StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    while let Some(row) = rows.next()? {
        if readable_by_gate(row.get_ref(1)?) && !visit(row.get(0)?) {
            break;
        }
    }
    Ok(())
}

/// 이전을 시작하기 전(아무 단계도 돌리기 전)에 경계 번호를 정한다. 규칙은
/// [`crate::Store::legacy_envelope_until`]의 문서 주석에 있다.
pub(super) fn boundary_before(conn: &Connection, version: u32) -> Result<Option<i64>, StoreError> {
    match version {
        // 새 파일.
        0 => Ok(None),
        // 시각 칸이 생기기 전: 그 시점의 마지막 번호. 이벤트가 없으면 0(옛 봉투 줄이 없다).
        1 | 2 => {
            let last: i64 =
                conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?;
            Ok(Some(last))
        }
        // 4차 이전에 버전 3이 된 파일.
        _ => v3_last_envelope(conn),
    }
}

/// 이미 버전 3인 파일의 옛 봉투 줄 가운데 가장 큰 번호.
fn v3_last_envelope(conn: &Connection) -> Result<Option<i64>, StoreError> {
    let mut last = None;
    scan_envelopes(conn, V3_FILLED_ENVELOPES, [], |seq| {
        last = Some(seq);
        false
    })?;
    Ok(last)
}

/// 모든 단계를 돌린 뒤: 경계 줄을 넣고 더 넣지 못하게 막는다.
pub(super) fn seal(conn: &Connection, boundary: Option<i64>) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO legacy_boundary (id, until_seq) VALUES (1, ?1)",
        [boundary],
    )?;
    conn.execute_batch(SEAL)?;
    Ok(())
}

/// 남긴 경계 번호. 경계 줄이 없으면(이 저장소가 버전 4로 올리지 않은 파일) 이상한 파일이다.
pub(crate) fn legacy_envelope_until(conn: &Connection) -> Result<Option<u64>, StoreError> {
    let row: Option<Option<i64>> = conn
        .query_row(
            "SELECT until_seq FROM legacy_boundary WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match row {
        None => Err(StoreError::Incompatible(
            "옛 기록 형식의 경계 줄이 없다".to_string(),
        )),
        Some(None) => Ok(None),
        Some(Some(seq)) => u64::try_from(seq).map(Some).map_err(|_| {
            StoreError::Incompatible(format!("옛 기록 형식의 경계 번호가 이상하다: {seq}"))
        }),
    }
}

/// 경계 번호 이하이면서 `after`보다 큰 옛 봉투 줄의 번호, 번호 순서로 `max`개까지(7차).
/// 규칙은 경계를 정할 때와 같은 [`envelope_condition`] · [`readable_by_gate`]다.
pub(crate) fn legacy_envelope_seqs(
    conn: &Connection,
    after: u64,
    max: usize,
) -> Result<Vec<u64>, StoreError> {
    let Some(until) = legacy_envelope_until(conn)? else {
        return Ok(Vec::new());
    };
    // 경계는 i64 범위 안이다. 그보다 큰 `after` 뒤에는 아무것도 없다.
    let (Ok(after), Ok(until)) = (i64::try_from(after), i64::try_from(until)) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    if max == 0 || after >= until {
        return Ok(out);
    }
    let mut bad = None;
    scan_envelopes(conn, ENVELOPES_BETWEEN, [after, until], |seq| {
        match u64::try_from(seq) {
            Ok(seq) => out.push(seq),
            Err(_) => bad = Some(seq),
        }
        bad.is_none() && out.len() < max
    })?;
    if let Some(seq) = bad {
        return Err(StoreError::Incompatible(format!(
            "이벤트 번호가 이상하다: {seq}"
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::{APPLICATION_ID, V1, V2, open};
    use super::legacy_envelope_until;

    /// 이벤트가 없는 옛 버전 파일을 만든다.
    fn empty_old_file(path: &std::path::Path, version: u32) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(V1).unwrap();
        if version >= 2 {
            conn.execute_batch(V2).unwrap();
        }
        conn.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
    }

    #[test]
    fn 이벤트가_없는_버전_1_2_파일의_경계는_0이다() {
        for version in [1, 2] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("pi.sqlite");
            empty_old_file(&path, version);
            let conn = open(&path).unwrap();
            assert_eq!(legacy_envelope_until(&conn).unwrap(), Some(0), "{version}");
        }
    }

    #[test]
    fn 경계_줄은_하나뿐이고_더_넣거나_바꿀_수_없다() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("pi.sqlite")).unwrap();
        for sql in [
            "INSERT INTO legacy_boundary (id, until_seq) VALUES (2, 5)",
            "INSERT OR REPLACE INTO legacy_boundary (id, until_seq) VALUES (1, 5)",
            "UPDATE legacy_boundary SET until_seq = 5",
            "DELETE FROM legacy_boundary",
        ] {
            assert!(conn.execute(sql, []).is_err(), "{sql}");
        }
        assert_eq!(legacy_envelope_until(&conn).unwrap(), None);
    }
}
