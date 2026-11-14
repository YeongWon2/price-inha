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

/// 이미 버전 3인 파일에서 경계로 보는 줄(옛 봉투 줄): 내용이 **정확히 두 칸**(`at` · `payload`)인
/// 객체이고, `at`이 정수이고, 시각 칸이 비었거나 내용의 `at`과 같다. 두 칸은 `json_each`로 센다.
/// `CASE`는 JSON이 아닌 내용에서 JSON 함수가 오류를 내지 않게 먼저 `json_valid`를 보려는 것이다.
/// (바로잡음 2026-11-14: 처음 SQL은 시각 칸이 채워진 줄만 보고 칸 수를 세지 않았다. 버전 4는 이번
/// 차수 안에서 아직 어느 실제 파일에도 쓰이지 않았으므로 버전 5를 따로 두지 않고 이 단계를 고친다.
/// 이 SQL은 버전 3 → 4로 올리는 순간에만 돌고, 이미 버전 4인 파일의 경계는 바뀌지 않는다.)
const V3_FILLED_ENVELOPES: &str = "
SELECT MAX(seq) FROM events
WHERE CASE WHEN json_valid(payload) THEN
      json_type(payload) = 'object'
      AND (SELECT COUNT(*) FROM json_each(payload)) = 2
      AND json_type(payload, '$.at') = 'integer'
      AND json_type(payload, '$.payload') IS NOT NULL
      AND (at IS NULL OR json_extract(payload, '$.at') = at)
  ELSE 0 END
";

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
        _ => Ok(conn.query_row(V3_FILLED_ENVELOPES, [], |r| r.get(0))?),
    }
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
