#![cfg(test)]
//! 6차 (5차 검토의 참고): 옛 기록 경계 번호를 정할 때 SQLite JSON 함수가 무엇을 돌려주는지와,
//! 짝 없는 서로게이트 이스케이프가 든 줄.
//!
//! - 번들 SQLite의 JSON 함수가 `-0` · i64 범위 밖의 정수 · 겹친 이름 · 짝 없는 서로게이트를 어떻게 보는지
//!   한 줄씩 고정한다. 번들 SQLite를 올렸을 때 경계 규칙이 흔들리면 원인이 이 테스트에서 바로 보인다.
//! - 짝 없는 서로게이트 이스케이프(`"\ud800"`)가 든 줄은 SQLite `json_valid`는 JSON으로 세지만
//!   `serde_json`(접속 관문)은 거부한다. 저장소도 관문과 같게 그 줄을 옛 봉투로 세지 않는다.
//!
//! `legacy_boundary.rs`(잠긴 파일)의 `handmade_texts`에는 더할 수 없어 이 파일에 따로 둔다.
//! `v3.db` 픽스처의 옛 봉투 줄은 2 · 3 · 7 · 10번이다(`legacy_boundary.rs` 머리말).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{SCHEMA_VERSION, Store};
use serde_json::{Value, json};

const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");

/// 픽스처를 임시 폴더에 복사한다. 원본은 건드리지 않는다.
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

/// 이벤트 줄을 저장소를 거치지 않고 넣는다(버전 3 파일의 이벤트 표는 넣기를 막지 않는다).
fn raw_insert(path: &Path, rows: &[(i64, &str, Option<i64>, &str)]) {
    let conn = rusqlite::Connection::open(path).unwrap();
    for (seq, kind, at, payload) in rows {
        conn.execute(
            "INSERT INTO events (seq, kind, payload, at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![seq, kind, payload, at],
        )
        .unwrap();
    }
}

fn boundary(path: &Path) -> Option<u64> {
    Store::open(path).unwrap().legacy_envelope_until().unwrap()
}

/// 번들 SQLite에 SQL 한 줄을 물어 글자로 받는다(빈 값은 `"NULL"`).
fn ask(conn: &rusqlite::Connection, sql: &str) -> String {
    conn.query_row(sql, [], |row| {
        let v: rusqlite::types::Value = row.get(0)?;
        Ok(match v {
            rusqlite::types::Value::Null => "NULL".to_string(),
            rusqlite::types::Value::Integer(i) => i.to_string(),
            rusqlite::types::Value::Real(f) => format!("real:{f}"),
            rusqlite::types::Value::Text(s) => s,
            rusqlite::types::Value::Blob(_) => "blob".to_string(),
        })
    })
    .unwrap()
}

// ───────────── 번들 SQLite의 JSON 함수가 돌려주는 것 (한 줄 단위) ─────────────

// 검증: D6, V6, C2, E3
#[test]
fn 번들_sqlite는_마이너스_0을_적힌_글자로_꺼내고_범위_밖_정수를_실수로_꺼낸다() {
    // 이 테스트는 저장소가 아니라 저장소가 기대는 번들 SQLite의 동작을 고정한다.
    // 경계 SQL은 아래 사실에 기대므로, 번들 SQLite를 올려 여기가 깨지면 경계 SQL을 다시 본다.
    // 저장소를 한 번 열어 번들 SQLite가 이 상자에 실제로 들어 있음을 함께 본다.
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.legacy_envelope_until().unwrap(), None);
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // `->`는 원래 적힌 글자를 돌려준다: `-0`은 `'-0'`이다.
    assert_eq!(ask(&conn, r#"SELECT '{"at":-0}' -> '$.at'"#), "-0");
    assert_eq!(ask(&conn, r#"SELECT '{"at":0}' -> '$.at'"#), "0");
    // `json_type`은 글자 모양만 본다: `-0`도 범위 밖의 정수도 `'integer'`다.
    assert_eq!(
        ask(&conn, r#"SELECT json_type('{"at":-0}', '$.at')"#),
        "integer"
    );
    assert_eq!(
        ask(
            &conn,
            r#"SELECT json_type('{"at":9223372036854775808}', '$.at')"#
        ),
        "integer"
    );
    // `json_extract`는 i64 범위 안의 정수는 SQL 정수로, 범위 밖은 실수로 꺼낸다.
    for (text, want) in [
        (r#"{"at":9223372036854775807}"#, "integer"),
        (r#"{"at":-9223372036854775808}"#, "integer"),
        (r#"{"at":0}"#, "integer"),
        (r#"{"at":9223372036854775808}"#, "real"),
        (r#"{"at":-9223372036854775809}"#, "real"),
        (r#"{"at":18446744073709551616}"#, "real"),
        (r#"{"at":1.0}"#, "real"),
    ] {
        assert_eq!(
            ask(
                &conn,
                &format!("SELECT typeof(json_extract('{text}', '$.at'))")
            ),
            want,
            "{text}"
        );
    }
    // 겹친 이름은 `json_each`가 하나씩 센다(두 칸 조건에서 빠진다).
    assert_eq!(
        ask(
            &conn,
            r#"SELECT COUNT(*) FROM json_each('{"at":1,"payload":1,"payload":2}')"#
        ),
        "3"
    );
}

// 검증: D6, V6, C2, E3
#[test]
fn 번들_sqlite의_json_valid는_짝_없는_서로게이트를_받고_serde_json은_거부한다() {
    // 두 쪽이 다르게 보는 입력이라 저장소가 따로 맞춰야 한다는 사실을 고정한다.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    for text in [
        r#"{"at":1,"payload":"\ud800"}"#,
        r#"{"at":1,"payload":"\udc00"}"#,
    ] {
        assert_eq!(
            ask(&conn, &format!("SELECT json_valid('{text}')")),
            "1",
            "SQLite: {text}"
        );
        assert!(
            serde_json::from_str::<Value>(text).is_err(),
            "serde_json: {text}"
        );
    }
    // 짝이 맞는 서로게이트는 두 쪽 모두 받는다.
    let pair = r#"{"at":1,"payload":"😀"}"#;
    assert_eq!(ask(&conn, &format!("SELECT json_valid('{pair}')")), "1");
    assert!(serde_json::from_str::<Value>(pair).is_ok());
}

// ───────────── 짝 없는 서로게이트가 든 줄은 옛 봉투가 아니다 ─────────────

/// 손으로 만든 서로게이트 JSON 글자와, 시각 칸이 비었을 때 옛 봉투로 세는지.
/// 셈은 `serde_json`(접속 관문)이 그 글자를 받는지와 같다(아래 테스트가 함께 확인한다).
fn surrogate_texts() -> Vec<(&'static str, bool)> {
    vec![
        // 짝 없는 앞 서로게이트 · 뒤 서로게이트
        (r#"{"at":1,"payload":"\ud800"}"#, false),
        (r#"{"at":1,"payload":"\udc00"}"#, false),
        (r#"{"at":1,"payload":"\uDBFF"}"#, false),
        (r#"{"at":1,"payload":"\uDFFF"}"#, false),
        // 글자 가운데 · 깊은 곳 · 배열 안
        (r#"{"at":1,"payload":{"t":"a\ud800b"}}"#, false),
        (r#"{"at":1,"payload":[1,["\udc00"]]}"#, false),
        // 앞 서로게이트 뒤에 뒤 서로게이트가 아닌 이스케이프 · 순서가 뒤집힌 짝
        (r#"{"at":1,"payload":"\ud800A"}"#, false),
        (r#"{"at":1,"payload":"\udc00\ud800"}"#, false),
        (r#"{"at":1,"payload":"\ud800\ud800"}"#, false),
        // 칸 이름 안
        (r#"{"at":1,"payload":{"\ud800":1}}"#, false),
        // 짝이 맞는 서로게이트 · 백슬래시를 이스케이프한 글자(서로게이트 아님)는 센다.
        (r#"{"at":1,"payload":"😀"}"#, true),
        (r#"{"at":1,"payload":"􏿿"}"#, true),
        (r#"{"at":1,"payload":"\\ud800"}"#, true),
        (r#"{"at":1,"payload":"A"}"#, true),
    ]
}

// 검증: D6, V6, C2, E3
#[test]
fn 손으로_만든_서로게이트_표는_접속_관문의_serde_json과_같게_센다() {
    for (text, counts) in surrogate_texts() {
        assert_eq!(
            serde_json::from_str::<Value>(text).is_ok(),
            counts,
            "표가 serde_json과 다르다: {text}"
        );
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 짝_없는_서로게이트가_든_줄은_옛_봉투로_세지_않아_경계를_키우지_않는다() {
    for (text, counts) in surrogate_texts() {
        // 시각 칸이 비었을 때와 내용의 `at`(1)과 같을 때 모두 본다.
        for column in [None, Some(1_i64)] {
            let (_dir, path) = copy_of(FIXTURE_V3);
            raw_insert(&path, &[(12, "feed_status", column, text)]);
            assert_eq!(raw_user_version(&path), 3);
            let want = if counts { Some(12) } else { Some(10) };
            let store = Store::open(&path).unwrap();
            assert_eq!(
                store.info().unwrap().schema_version,
                SCHEMA_VERSION,
                "{text}"
            );
            assert_eq!(
                store.legacy_envelope_until().unwrap(),
                want,
                "시각 칸 {column:?}, 내용 {text}"
            );
            drop(store);
            assert_eq!(boundary(&path), want, "다시 열기: {column:?} {text}");
        }
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 서로게이트_줄_앞뒤의_옛_봉투_줄이_경계를_정한다() {
    let lone = r#"{"at":1,"payload":"\ud800"}"#;
    let envelope = json!({ "at": t(3000).0, "payload": { "ok": true } }).to_string();

    // 12번 옛 봉투, 13번 서로게이트 줄 → 경계는 12다.
    let (_dir, path) = copy_of(FIXTURE_V3);
    raw_insert(
        &path,
        &[
            (12, "feed_status", None, envelope.as_str()),
            (13, "feed_status", None, lone),
        ],
    );
    assert_eq!(boundary(&path), Some(12));
    assert_eq!(boundary(&path), Some(12), "다시 열어도 같다");

    // 12번 서로게이트 줄, 13번 옛 봉투 → 경계는 13이다.
    let (_dir2, path2) = copy_of(FIXTURE_V3);
    raw_insert(
        &path2,
        &[
            (12, "feed_status", Some(1), lone),
            (13, "feed_status", None, envelope.as_str()),
        ],
    );
    assert_eq!(boundary(&path2), Some(13));

    // 12 · 13 · 14번이 모두 서로게이트 줄이면 v3.db의 옛 봉투 줄(10번)이 경계다.
    let (_dir3, path3) = copy_of(FIXTURE_V3);
    raw_insert(
        &path3,
        &[
            (12, "feed_status", None, lone),
            (13, "feed_status", Some(1), r#"{"at":1,"payload":"\udc00"}"#),
            (
                14,
                "feed_status",
                None,
                r#"{"at":1,"payload":{"\uDBFF":0}}"#,
            ),
        ],
    );
    assert_eq!(boundary(&path3), Some(10));
}
