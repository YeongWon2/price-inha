#![cfg(test)]
//! 4차 1번 · 4번: 스키마 버전 4.
//!
//! - 1번: 이벤트 시각 칸(`events.at`)에 색인이 있어 시각 범위 읽기가 표 전체를 훑지 않는다
//!   (3차 검토: 30만 줄에서 색인 없이 약 10ms, 그동안 쓰기가 기다린다).
//!   시간을 재는 대신 SQLite의 `EXPLAIN QUERY PLAN`으로 색인을 쓰는지 본다.
//! - 4번: 이전 도중에 실패하면 버전 · 보호 트리거 · 경계가 이전 전 그대로 남는다.
//!
//! 공개 API로는 색인과 트리거를 볼 수 없어서 저장소 파일을 `rusqlite`로 직접 열어 본다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{EventRange, Store};
use serde_json::json;

const FIXTURE_V1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db");
const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");
const FIXTURE_V3_PLAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3_plain.db");

fn copy_of(fixture: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(fixture, &path).unwrap();
    (dir, path)
}

fn raw_user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// 새 파일과 픽스처들을 열어 버전 4로 만든 파일들. (이름, 폴더, 경로)
fn upgraded_files() -> Vec<(&'static str, tempfile::TempDir, PathBuf)> {
    let mut out = Vec::new();
    let (dir, path, store) = temp_store();
    store.append_at("chat", &json!({ "n": 1 }), t(1)).unwrap();
    drop(store);
    out.push(("새 파일", dir, path));
    for (name, fixture) in [
        ("버전 1", FIXTURE_V1),
        ("버전 2", FIXTURE_V2),
        ("버전 3", FIXTURE_V3),
        ("버전 3 봉투 없음", FIXTURE_V3_PLAIN),
    ] {
        let (dir, path) = copy_of(fixture);
        drop(Store::open(&path).unwrap());
        out.push((name, dir, path));
    }
    out
}

/// `events` 표의 색인 가운데 첫 칸이 `at`인 것의 이름.
fn at_indexes(conn: &rusqlite::Connection) -> Vec<String> {
    let names: Vec<String> = conn
        .prepare("SELECT name FROM pragma_index_list('events')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    names
        .into_iter()
        .filter(|name| {
            let first: Option<String> = conn
                .query_row(
                    "SELECT name FROM pragma_index_info(?1) WHERE seqno = 0",
                    [name],
                    |row| row.get(0),
                )
                .ok();
            first.as_deref() == Some("at")
        })
        .collect()
}

/// 질의의 `EXPLAIN QUERY PLAN` 설명 줄들.
fn plan(conn: &rusqlite::Connection, sql: &str) -> Vec<String> {
    conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// 검증: D6, C2, E3
#[test]
fn 버전_4_파일의_이벤트_시각_칸에_색인이_있다() {
    for (name, _dir, path) in upgraded_files() {
        assert_eq!(raw_user_version(&path), 4, "{name}");
        let conn = rusqlite::Connection::open(&path).unwrap();
        assert!(
            !at_indexes(&conn).is_empty(),
            "{name}: events(at) 색인이 없다"
        );
    }
}

// 검증: D6, C2, E3
#[test]
fn 시각_범위로_읽는_질의는_표를_훑지_않고_시각_색인을_쓴다() {
    for (name, _dir, path) in upgraded_files() {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let indexes = at_indexes(&conn);
        // 시각 범위 읽기의 모양들: 시각 두 끝, 한 끝, 번호 조건과 함께.
        for sql in [
            "SELECT seq, kind, at, payload FROM events WHERE at > 1 AND at < 5 ORDER BY seq LIMIT 100",
            "SELECT seq, kind, at, payload FROM events WHERE seq > 0 AND at > 1 AND at < 5 ORDER BY seq LIMIT 100",
            "SELECT seq FROM events WHERE at > 1",
        ] {
            let lines = plan(&conn, sql);
            assert!(
                lines
                    .iter()
                    .any(|line| line.starts_with("SEARCH events USING")
                        && indexes.iter().any(|index| line.contains(index.as_str()))
                        && line.contains("at>?")),
                "{name}: 시각 색인을 쓰지 않는다: {sql}\n{lines:?}"
            );
            assert!(
                !lines.iter().any(|line| line.starts_with("SCAN events")),
                "{name}: 표 전체를 훑는다: {sql}\n{lines:?}"
            );
        }
    }
}

// 검증: D6, C2, E3
#[test]
fn 색인이_생겨도_시각_범위_읽기의_결과와_순서는_그대로다() {
    let (_dir, _path, store) = temp_store();
    // 시각이 번호 순서와 다르게 섞여 있다.
    for (n, at) in [
        (1, Some(50)),
        (2, None),
        (3, Some(10)),
        (4, Some(30)),
        (5, Some(20)),
    ] {
        match at {
            Some(at) => store.append_at("chat", &json!({ "n": n }), t(at)).unwrap(),
            None => store.append("chat", &json!({ "n": n })).unwrap(),
        };
    }
    let got: Vec<u64> = store
        .read_events(
            EventRange {
                after_seq: 0,
                after_at: Some(t(10)),
                before_at: Some(t(50)),
            },
            100,
        )
        .unwrap()
        .into_iter()
        .map(|e| e.seq)
        .collect();
    // 번호 순서다(시각 순서가 아니다). 끝은 들지 않는다.
    assert_eq!(got, vec![4, 5]);
}

/// 트리거의 (이름, SQL) 목록.
fn triggers(path: &Path) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.prepare("SELECT name, sql FROM sqlite_master WHERE type = 'trigger' ORDER BY name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn has_events_at(path: &Path) -> bool {
    let conn = rusqlite::Connection::open(path).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('events') WHERE name = 'at'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    n == 1
}

// 검증: D6, V6, C2, E3, M6, M1
#[test]
fn 이전이_도중에_실패하면_버전과_보호_트리거가_그대로이고_고친_뒤_다시_열면_올라간다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let before = triggers(&path);
    assert!(before.iter().any(|(name, _)| name == "events_no_update"));
    // 이 시험만의 트리거가 이전 도중(시각을 옮겨 적는 UPDATE)에 실패하게 한다.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER test_fail_backfill BEFORE UPDATE ON events
             BEGIN SELECT RAISE(ABORT, 'test: fail in the middle'); END;",
        )
        .unwrap();
    }
    for _ in 0..2 {
        assert!(Store::open(&path).is_err());
        // 버전 · 칸 · 보호 트리거가 이전 전 그대로다(내려간 보호 트리거가 없다).
        assert_eq!(raw_user_version(&path), 2);
        assert!(!has_events_at(&path));
        let now: Vec<(String, String)> = triggers(&path)
            .into_iter()
            .filter(|(name, _)| name != "test_fail_backfill")
            .collect();
        assert_eq!(now, before);
        // 보호가 그대로 막는다.
        let conn = rusqlite::Connection::open(&path).unwrap();
        assert!(conn.execute("DELETE FROM events", []).is_err());
        assert!(conn.execute("DELETE FROM access_log", []).is_err());
        assert!(conn.execute("DELETE FROM team_history", []).is_err());
    }

    // 시험용 트리거를 걷어 내면 올라가고, 경계는 처음 올린 것처럼 정해진다.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("DROP TRIGGER test_fail_backfill")
            .unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 4);
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(7));
    assert_eq!(store.read_after(0, 100).unwrap().len(), 7);
    drop(store);
    // 보호 트리거는 버전 2 때의 것이 모두 남아 있다(글자까지 같다).
    let after = triggers(&path);
    for trigger in &before {
        assert!(
            after.contains(trigger),
            "보호 트리거가 바뀌었다: {trigger:?}"
        );
    }
}
