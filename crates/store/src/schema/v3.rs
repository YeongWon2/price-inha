//! 스키마 버전 2 → 3(3차 작업, 대조표 C8): 이벤트 시각 칸.
//!
//! - `events`에 시각 칸 `at`(정수, 비어 있어도 된다)을 더한다. 버전 2까지의 줄과
//!   [`crate::Store::append`]로 쌓은 줄은 비어 있다.
//! - 이미 있는 줄 가운데 내용이 `{"at": 정수, "payload": …}` 모양(접속 관문이 쓰는 봉투:
//!   객체이고, `at`이 정수이고, `payload` 키가 있다 — 값이 `null`이어도 된다)인 줄은 그 `at`을
//!   시각 칸에 옮겨 적는다. 내용 · 종류 · 번호는 건드리지 않는다.
//!
//! 기록 보호(버전 1의 `events_no_update`)가 이 옮겨 적기를 막으므로, 이 단계 안에서만 그 트리거를
//! 내렸다가 시각 칸만 채우고 **같은 글자의** 트리거를 다시 건다. 이 단계는
//! [`super::migrate`]의 한 트랜잭션 안에서 버전 표시와 함께 돌므로, 중간에 실패하거나 꺼지면
//! 트리거가 내려간 상태로 남지 않고 파일은 버전 2 그대로다. 트리거를 내리는 SQL은 여기에만 있고
//! 공개 경로가 없다. 이미 나간 버전 1 · 2의 SQL은 고치지 않는다.
//!
//! `CASE`로 감싼 것은 SQL이 `AND`를 앞에서부터 끊어 계산한다는 보장이 없어서다: JSON이 아닌
//! 내용(SQL로 직접 넣은 줄)에서 JSON 함수가 오류를 내지 않게 먼저 `json_valid`를 본다.
pub(super) const V3: &str = "
ALTER TABLE events ADD COLUMN at INTEGER;
DROP TRIGGER events_no_update;
UPDATE events SET at = json_extract(payload, '$.at')
WHERE CASE WHEN json_valid(payload) THEN
    json_type(payload) = 'object'
    AND json_type(payload, '$.at') = 'integer'
    AND json_type(payload, '$.payload') IS NOT NULL
ELSE 0 END;
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
";

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::{APPLICATION_ID, V1, V2, open, user_version};

    fn temp_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi.sqlite");
        (dir, path)
    }

    /// 버전 2 스키마만 가진 파일을 만든다.
    fn v2_file(path: &std::path::Path, extra_sql: &str) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(V1).unwrap();
        conn.execute_batch(V2).unwrap();
        conn.execute_batch(extra_sql).unwrap();
        conn.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
    }

    fn trigger_sql(conn: &Connection) -> Option<String> {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'events_no_update'",
            [],
            |r| r.get(0),
        )
        .ok()
    }

    #[test]
    fn 다시_건_보호_트리거는_버전_1의_것과_글자까지_같다() {
        let (_dir, path) = temp_path();
        v2_file(&path, "");
        let before = trigger_sql(&Connection::open(&path).unwrap()).unwrap();
        let conn = open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 4);
        assert_eq!(trigger_sql(&conn).unwrap(), before);
        // 새로 만든 파일도 같은 트리거다.
        let (_dir2, fresh) = temp_path();
        assert_eq!(trigger_sql(&open(&fresh).unwrap()).unwrap(), before);
    }

    #[test]
    fn json이_아닌_내용의_줄이_있어도_이전이_되고_시각은_비어_있다() {
        let (_dir, path) = temp_path();
        v2_file(
            &path,
            "INSERT INTO events (seq, kind, payload) VALUES
                 (1, 'chat', 'not json'),
                 (2, 'chat', '[1, 2]'),
                 (3, 'chat', '{\"at\": 5, \"payload\": null}'),
                 (4, 'chat', '{\"at\": 5.0, \"payload\": 1}');",
        );
        let conn = open(&path).unwrap();
        let rows: Vec<(i64, Option<i64>, String)> = conn
            .prepare("SELECT seq, at, payload FROM events ORDER BY seq")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let ats: Vec<Option<i64>> = rows.iter().map(|r| r.1).collect();
        assert_eq!(ats, vec![None, None, Some(5), None]);
        assert_eq!(rows[0].2, "not json");
    }

    #[test]
    fn 이전이_트리거를_내린_뒤에_실패하면_버전_2와_보호_트리거가_그대로_남는다() {
        let (_dir, path) = temp_path();
        // 이 시험만의 트리거가 옮겨 적기(트리거를 내린 뒤의 UPDATE)를 실패하게 한다.
        v2_file(
            &path,
            "INSERT INTO events (seq, kind, payload) VALUES (1, 'chat', '{\"at\": 7, \"payload\": 1}');
             CREATE TRIGGER test_fail_backfill BEFORE UPDATE ON events
             BEGIN SELECT RAISE(ABORT, 'test: fail in the middle'); END;",
        );
        let before = trigger_sql(&Connection::open(&path).unwrap()).unwrap();
        assert!(open(&path).is_err());

        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 2);
        assert_eq!(trigger_sql(&conn).unwrap(), before);
        let has_at: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('events') WHERE name = 'at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_at, 0);
        // 시험용 트리거를 걷어 내도 버전 1의 보호가 그대로 막는다.
        conn.execute_batch("DROP TRIGGER test_fail_backfill")
            .unwrap();
        assert!(conn.execute("UPDATE events SET kind = 'x'", []).is_err());
    }
}
