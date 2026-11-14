#![cfg(test)]
//! 4차 5번 (2026-11-14 사용자 결정 "경계 번호로"): 옛 기록 형식의 경계 번호.
//!
//! 버전 3 이전에는 접속 관문이 이벤트 내용을 `{"at", "payload"}` 봉투로 싸서 쌓았다. 버전을 올릴 때
//! 경계 번호를 한 번 정해 남기고 [`Store::legacy_envelope_until`]로 읽는다. 정하는 규칙:
//!
//! - 처음부터 새로 만든 파일: 없음.
//! - 버전 1 · 2 파일을 올릴 때: 그 시점의 마지막 이벤트 번호.
//! - 이미 버전 3인 파일(4차 이전에 올린 파일): 옛 봉투 줄 가운데 가장 큰 번호. 옛 봉투 줄은 접속 관문
//!   4차(`crates/gate/src/frame.rs`의 `time_and_payload`)가 푸는 모양과 같다: 내용이 정확히 두 칸
//!   (`at` · `payload`)인 객체이고, `at`이 정수이고, 시각 칸이 비었거나 내용의 `at`과 같다.
//!   (바로잡음 2026-11-14: 처음 규칙 "시각 칸이 채워진 봉투 줄"은 시각 칸이 빈 봉투 줄을 빠뜨렸다.)
//!
//! 픽스처(모두 WAL을 체크포인트해 파일 하나로 두었다. 테스트는 임시 폴더에 복사해서 연다):
//!
//! - `v1.db`: 버전 1. 이벤트 3개(봉투 모양 줄 없음).
//! - `v2.db`: 버전 2. 이벤트 7개. 봉투 모양은 2 · 3 · 7번(`migrate_v3.rs` 머리말).
//! - `v3.db`: 4차를 시작하기 전의 코드(스키마 버전 3)로 `v2.db`를 열어 3으로 올린 뒤 이벤트 4개를 더 쌓은 것.
//!   - 8번 `append`(봉투 아님, 시각 없음) · 9번 `append_at`(봉투 아님) ·
//!     10번 `append_at`(내용이 봉투 모양이고 내용의 `at`과 시각 칸이 같다) · 11번 `append_at`(봉투 아님).
//!   - 옛 봉투 줄은 2 · 3 · 7 · 10번이다(모두 시각 칸이 채워져 있다). 10번은 옛 봉투가 아니라 새로 쌓은 줄이지만
//!     파일만 보고는 버전 2에서 옮겨 적은 줄과 가를 수 없다. 지시서의 규칙대로 경계는 10이다
//!     (한계: 이 줄은 경계 안에 든다).
//! - `v3_plain.db`: 4차를 시작하기 전의 코드로 처음부터 새로 만든 버전 3 파일. 이벤트 4개.
//!   - 1번 `append` · 2번 `append_at` · 3번 `append`(내용은 봉투 모양이지만 시각 칸이 비어 있다) ·
//!     4번 `append_at`. 3번은 시각 칸이 빈 옛 봉투 줄이라 경계는 3이다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{EventRange, Store, TeamState, TimedEvent, TradeId, TradeState, UnixMillis};
use serde_json::{Value, json};

const FIXTURE_V1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db");
const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");
const FIXTURE_V3_PLAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3_plain.db");

/// 픽스처를 임시 폴더에 복사한다. 원본은 건드리지 않는다.
fn copy_of(fixture: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(fixture, &path).unwrap();
    (dir, path)
}

/// 파일에 적힌 스키마 버전을 저장소를 거치지 않고 읽는다.
fn raw_user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// 저장소를 거치지 않고 읽은 이벤트 줄: (번호, 종류, 시각 칸, 내용 글자).
type RawEvent = (i64, String, Option<i64>, String);

fn raw_events(path: &Path) -> Vec<RawEvent> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.prepare("SELECT seq, kind, at, payload FROM events ORDER BY seq")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn all() -> EventRange {
    EventRange::default()
}

fn boundary(path: &Path) -> Option<u64> {
    Store::open(path).unwrap().legacy_envelope_until().unwrap()
}

/// `v3.db`에 든 이벤트: (종류, 시각, 내용). 번호는 순서대로 1부터다.
fn v3_event_rows() -> Vec<(&'static str, Option<UnixMillis>, Value)> {
    vec![
        ("chat", None, json!({ "n": 1, "text": "장 시작" })),
        (
            "feed_status",
            Some(t(1000)),
            json!({ "at": t(1000).0, "payload": { "ok": true } }),
        ),
        (
            "Old.Kind",
            Some(t(1010)),
            json!({ "at": t(1010).0, "payload": null }),
        ),
        ("quotes stale", None, json!("글자 하나만")),
        ("chat", None, json!({ "at": "어제", "payload": 1 })),
        ("chat", None, json!({ "at": t(1020).0 })),
        (
            "trade",
            Some(t(990)),
            json!({ "at": t(990).0, "payload": { "amount": 9_007_199_254_740_993_i64 } }),
        ),
        ("chat", None, json!({ "n": 8, "text": "올린 뒤" })),
        ("feed_status", Some(t(2000)), json!({ "ok": false })),
        (
            "feed_status",
            Some(t(2010)),
            json!({ "at": t(2010).0, "payload": { "ok": true } }),
        ),
        ("chat", Some(t(2020)), json!({ "n": 11, "text": "마지막" })),
    ]
}

fn v3_timed_events() -> Vec<TimedEvent> {
    v3_event_rows()
        .into_iter()
        .enumerate()
        .map(|(i, (kind, at, payload))| TimedEvent {
            seq: i as u64 + 1,
            kind: kind.to_string(),
            at,
            payload,
        })
        .collect()
}

/// 봉투 모양: 객체이고, `at`이 정수이고, `payload` 키가 있다.
fn is_envelope(payload: &Value) -> bool {
    payload.as_object().is_some_and(|o| {
        o.get("at").is_some_and(|at| at.is_i64() || at.is_u64()) && o.contains_key("payload")
    })
}

// ───────────── 픽스처가 정말 그 모양인지 ─────────────

// 검증: D6, V6, C2, E3
#[test]
fn 버전_3_픽스처는_버전_3이고_봉투_줄과_시각이_머리말대로다() {
    for (fixture, events) in [(FIXTURE_V3, 11), (FIXTURE_V3_PLAIN, 4)] {
        let (_dir, path) = copy_of(fixture);
        assert_eq!(raw_user_version(&path), 3, "{fixture}");
        let rows = raw_events(&path);
        assert_eq!(rows.len(), events, "{fixture}");
        // 번호는 1부터 빠짐없다.
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.0, i as i64 + 1);
        }
    }
    // v3.db: 시각 칸이 채워진 봉투 모양 줄은 2 · 3 · 7 · 10번이고, 10번은 내용의 `at`과 시각 칸이 같다.
    let (_dir, path) = copy_of(FIXTURE_V3);
    let filled_envelopes: Vec<i64> = raw_events(&path)
        .into_iter()
        .filter(|(_, _, at, payload)| {
            at.is_some() && is_envelope(&serde_json::from_str(payload).unwrap())
        })
        .map(|(seq, ..)| seq)
        .collect();
    assert_eq!(filled_envelopes, vec![2, 3, 7, 10]);
    let tenth = &raw_events(&path)[9];
    let tenth_payload: Value = serde_json::from_str(&tenth.3).unwrap();
    assert_eq!(tenth.2, tenth_payload["at"].as_i64());

    // v3_plain.db: 봉투 모양 줄(3번)은 시각 칸이 비어 있다.
    let (_dir, path) = copy_of(FIXTURE_V3_PLAIN);
    let rows = raw_events(&path);
    let envelopes: Vec<(i64, Option<i64>)> = rows
        .iter()
        .filter(|(_, _, _, payload)| is_envelope(&serde_json::from_str(payload).unwrap()))
        .map(|(seq, _, at, _)| (*seq, *at))
        .collect();
    assert_eq!(envelopes, vec![(3, None)]);
}

// ───────────── 경계를 정하는 규칙 ─────────────

// 검증: D6, C2, E3
#[test]
fn 새로_만든_파일은_경계가_없다() {
    let (_dir, path, store) = temp_store();
    assert_eq!(store.legacy_envelope_until().unwrap(), None);
    // 봉투 모양 내용을 시각과 함께 · 시각 없이 쌓아도 경계는 생기지 않는다.
    store
        .append_at(
            "feed_status",
            &json!({ "at": t(5).0, "payload": { "ok": true } }),
            t(5),
        )
        .unwrap();
    store
        .append("feed_status", &json!({ "at": t(6).0, "payload": 1 }))
        .unwrap();
    store.append_at("chat", &json!({ "n": 3 }), t(7)).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), None);
    drop(store);
    assert_eq!(boundary(&path), None);
}

// 검증: D6, V6, C2, E3
#[test]
fn 버전_2_파일을_올리면_경계는_올리기_직전의_마지막_이벤트_번호다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    assert_eq!(raw_user_version(&path), 2);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.read_after(0, 100).unwrap().len(), 7);
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(7));
}

// 검증: D6, V6, C2, E3
#[test]
fn 버전_1_파일을_올리면_봉투_줄이_없어도_경계는_마지막_이벤트_번호다() {
    let (_dir, path) = copy_of(FIXTURE_V1);
    assert_eq!(raw_user_version(&path), 1);
    let store = Store::open(&path).unwrap();
    let events = store.read_events(all(), 100).unwrap();
    assert_eq!(events.len(), 3);
    // 버전 1 파일에는 봉투 모양 줄이 없다(시각이 모두 없다).
    assert!(events.iter().all(|e| e.at.is_none()));
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(3));
}

// 검증: D6, V6, C2, E3
#[test]
fn 이미_버전_3인_파일은_옛_봉투_줄_가운데_가장_큰_번호가_경계다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    // 10번은 버전 3에서 `append_at`으로 새로 쌓은 봉투 모양 줄이다. 지시서의 규칙은 이 줄을
    // 옛 봉투와 가르지 못하므로 경계는 마지막 옛 줄(7)이 아니라 10이다. 11번(봉투 아님)은 경계 밖이다.
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
}

// 검증: D6, V6, C2, E3
#[test]
fn 시각_칸이_빈_봉투_줄만_있는_버전_3_파일도_그_줄이_경계다() {
    let (_dir, path) = copy_of(FIXTURE_V3_PLAIN);
    let store = Store::open(&path).unwrap();
    // 3번은 시각 칸이 빈 봉투 줄이다(접속 관문 3차가 `append`로 쌓던 모양). 접속 관문 4차는 이 줄을
    // 옛 봉투로 풀므로 경계 안에 든다. 4번(봉투 아님)은 경계 밖이다.
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(3));
}

// 검증: D6, V6, M1, M2, M5, C2, E3, M6
#[test]
fn 버전_3_파일을_4로_올려도_자료와_시각이_그대로다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let before = raw_events(&path);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 4);
    assert_eq!(store.read_events(all(), 100).unwrap(), v3_timed_events());
    // 내용 글자도 그대로다(저장소를 거치지 않고 견준다).
    drop(store);
    assert_eq!(raw_events(&path), before);
    assert_eq!(raw_user_version(&path), 4);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
    assert_eq!(store.team_history().unwrap().len(), 3);
    assert_eq!(store.access_log().unwrap().len(), 2);
    assert_eq!(
        store.get_trade(TradeId(2)).unwrap().state,
        TradeState::Blocked
    );
    assert_eq!(
        store.get_trade(TradeId(3)).unwrap().state,
        TradeState::AwaitingApproval
    );
    // 번호는 이어진다.
    assert_eq!(store.append("chat", &json!({ "n": 12 })).unwrap(), 12);
}

// ───────────── 경계는 바뀌지 않는다 ─────────────

/// (이름, 파일을 만드는 방법, 기대하는 경계)
fn cases() -> Vec<(&'static str, Option<&'static str>, Option<u64>)> {
    vec![
        ("새 파일", None, None),
        ("버전 1", Some(FIXTURE_V1), Some(3)),
        ("버전 2", Some(FIXTURE_V2), Some(7)),
        ("버전 3", Some(FIXTURE_V3), Some(10)),
        ("버전 3 시각 칸이 빈 봉투", Some(FIXTURE_V3_PLAIN), Some(3)),
    ]
}

fn file_for(fixture: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    match fixture {
        Some(fixture) => copy_of(fixture),
        None => {
            let (dir, path, store) = temp_store();
            drop(store);
            (dir, path)
        }
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 경계는_다시_열거나_이벤트를_더_쌓아도_바뀌지_않는다() {
    for (name, fixture, want) in cases() {
        let (_dir, path) = file_for(fixture);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.legacy_envelope_until().unwrap(), want, "{name}");
        let last = store.read_after(0, 1_000).unwrap().len() as u64;

        // 봉투 모양 줄을 시각과 함께 · 시각 없이, 봉투 아닌 줄도 쌓는다.
        for n in 1..=3_i64 {
            store
                .append_at(
                    "feed_status",
                    &json!({ "at": t(5_000 + n).0, "payload": { "n": n } }),
                    t(5_000 + n),
                )
                .unwrap();
            store
                .append(
                    "feed_status",
                    &json!({ "at": t(6_000 + n).0, "payload": n }),
                )
                .unwrap();
            store
                .append_at("chat", &json!({ "n": n }), t(7_000 + n))
                .unwrap();
        }
        assert_eq!(
            store.read_after(0, 1_000).unwrap().len() as u64,
            last + 9,
            "{name}"
        );
        assert_eq!(store.legacy_envelope_until().unwrap(), want, "{name}");
        // 복제한 손잡이도 같다.
        assert_eq!(
            store.clone().legacy_envelope_until().unwrap(),
            want,
            "{name}"
        );
        drop(store);

        for _ in 0..3 {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.info().unwrap().schema_version, 4, "{name}");
            assert_eq!(store.legacy_envelope_until().unwrap(), want, "{name}");
        }
    }
}

// 검증: D6, V6
#[test]
fn 버전_2_파일을_닫지_못하고_꺼져도_정한_경계가_남는다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(7));
    store.append("chat", &json!({ "n": 8 })).unwrap();
    std::mem::forget(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(7));
}

/// 저장소 파일의 모든 표에 고치기 · 지우기 · 다시 넣기 · 갈아 끼우기를 해 본다.
/// 경계를 어느 표 어느 칸에 두든 모두 건드려 보기 위해서다. 막혀서 나는 오류는 상관없다.
/// 실제로 들어간 SQL이 하나도 없으면(SQL이 틀렸으면) 멈춘다.
fn tamper_every_table(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut sqls: Vec<String> = Vec::new();
    for table in &tables {
        let columns: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let list = columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        for column in &columns {
            for value in ["0", "1", "12345", "NULL", "'x'"] {
                sqls.push(format!("UPDATE \"{table}\" SET \"{column}\" = {value}"));
                sqls.push(format!(
                    "UPDATE OR REPLACE \"{table}\" SET \"{column}\" = {value}"
                ));
            }
            sqls.push(format!(
                "UPDATE \"{table}\" SET \"{column}\" = \"{column}\" + 1"
            ));
            sqls.push(format!(
                "UPDATE \"{table}\" SET \"{column}\" = \"{column}\" - 1"
            ));
            sqls.push(format!(
                "INSERT INTO \"{table}\" ({list}) SELECT {list} FROM \"{table}\" WHERE true
                 ON CONFLICT DO UPDATE SET \"{column}\" = 12345"
            ));
        }
        sqls.push(format!(
            "INSERT OR REPLACE INTO \"{table}\" ({list}) SELECT {list} FROM \"{table}\""
        ));
        sqls.push(format!("DELETE FROM \"{table}\""));
    }
    let mut applied = 0;
    for sql in &sqls {
        if conn.execute(sql, []).is_ok() {
            applied += 1;
        }
    }
    // 매매 표처럼 고칠 수 있는 표가 있으므로 들어간 SQL이 있어야 한다.
    assert!(applied > 0, "들어간 SQL이 없다: SQL이 틀렸다");
}

// 검증: D6, V6, C2, E3
#[test]
fn 경계는_저장소_파일에_직접_넣은_sql로도_바뀌지_않는다() {
    for (name, fixture, want) in cases() {
        let (_dir, path) = file_for(fixture);
        assert_eq!(boundary(&path), want, "{name}");
        tamper_every_table(&path);
        assert_eq!(boundary(&path), want, "{name}");
        assert_eq!(raw_user_version(&path), 4, "{name}");
    }
}

// ───────────── 옛 봉투 줄의 모양 (접속 관문 4차와 같은 규칙) ─────────────

/// 접속 관문 4차(`crates/gate/src/frame.rs`의 `time_and_payload`)가 옛 봉투로 푸는 줄인지.
/// `pi-gate`에 기대지 않고 규칙을 여기에 그대로 적는다: 내용이 정확히 두 칸(`at` · `payload`)인
/// 객체이고, `at`이 정수이고, 시각 칸이 비었거나 내용의 `at`과 같다.
fn gate_unwraps(column: Option<i64>, payload: &str) -> bool {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(payload) else {
        return false;
    };
    map.len() == 2
        && map.contains_key("payload")
        && map.get("at").is_some_and(Value::is_i64)
        && column.is_none_or(|at| map.get("at") == Some(&Value::from(at)))
}

/// 이벤트 줄을 저장소를 거치지 않고 넣는다. 버전 3 파일의 이벤트 표는 넣기를 막지 않는다
/// (같은 번호로 다시 넣기만 막는다).
fn raw_insert(path: &Path, rows: &[(i64, &str, Option<i64>, String)]) {
    let conn = rusqlite::Connection::open(path).unwrap();
    for (seq, kind, at, payload) in rows {
        conn.execute(
            "INSERT INTO events (seq, kind, payload, at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![seq, kind, payload, at],
        )
        .unwrap();
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 버전_3_파일에_시각_칸이_빈_봉투_줄을_더하고_올리면_그_줄이_경계_안에_든다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    assert_eq!(raw_user_version(&path), 3);
    // 12번: 접속 관문 3차가 `append`로 쌓던 모양(시각 칸이 비고 내용이 봉투). 13번: 봉투 아님.
    let twelfth = json!({ "at": t(3000).0, "payload": { "ok": true } }).to_string();
    let thirteenth = json!({ "n": 13 }).to_string();
    raw_insert(
        &path,
        &[
            (12, "feed_status", None, twelfth.clone()),
            (13, "chat", None, thirteenth),
        ],
    );
    assert!(gate_unwraps(None, &twelfth));
    assert_eq!(raw_user_version(&path), 3);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 4);
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(12));
    // 올려도 12번은 시각 칸이 빈 채 내용 그대로다.
    let events = store.read_events(all(), 100).unwrap();
    assert_eq!(events.len(), 13);
    assert_eq!(events[11].at, None);
    assert_eq!(
        events[11].payload,
        json!({ "at": t(3000).0, "payload": { "ok": true } })
    );
    drop(store);
    assert_eq!(boundary(&path), Some(12));
}

// 검증: D6, V6, C2, E3
#[test]
fn 세_칸_이상인_객체는_옛_봉투가_아니라_경계를_키우지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let three = json!({ "at": 77, "payload": 1, "extra": true }).to_string();
    // 12번: 시각 칸이 내용의 `at`과 같다. 13번: 시각 칸이 비었다. 둘 다 세 칸이라 옛 봉투가 아니다.
    raw_insert(
        &path,
        &[
            (12, "feed_status", Some(77), three.clone()),
            (13, "feed_status", None, three.clone()),
        ],
    );
    assert!(!gate_unwraps(Some(77), &three));
    assert!(!gate_unwraps(None, &three));

    let store = Store::open(&path).unwrap();
    // v3.db의 옛 봉투 줄은 2 · 3 · 7 · 10번이다.
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
}

/// `v3_plain.db`와 표 · 트리거 · 다른 표의 자료 · 버전 · 파일 표시가 같고 이벤트만 없는 버전 3 파일을 만든다.
fn empty_v3_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.sqlite");
    fs::copy(FIXTURE_V3_PLAIN, &src).unwrap();
    let path = dir.path().join("pi.sqlite");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("ATTACH DATABASE ?1 AS src", [src.to_str().unwrap()])
        .unwrap();
    let objects: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT type, name, sql FROM src.sqlite_master
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // 표를 먼저 만들고, 이벤트 말고 다른 표의 자료를 옮긴 뒤, 색인과 트리거를 만든다.
    for (kind, _, sql) in &objects {
        if kind == "table" {
            conn.execute_batch(sql).unwrap();
        }
    }
    for (kind, name, _) in &objects {
        if kind == "table" && name != "events" {
            conn.execute_batch(&format!(
                "INSERT INTO main.\"{name}\" SELECT * FROM src.\"{name}\""
            ))
            .unwrap();
        }
    }
    for (kind, _, sql) in &objects {
        if kind != "table" {
            conn.execute_batch(sql).unwrap();
        }
    }
    let version: i64 = conn
        .query_row("PRAGMA src.user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 3);
    // 저장소 파일 표시(`application_id`)도 같게 둔다.
    let app_id: i64 = conn
        .query_row("PRAGMA src.application_id", [], |r| r.get(0))
        .unwrap();
    conn.execute_batch(&format!(
        "DETACH DATABASE src; PRAGMA user_version = 3; PRAGMA application_id = {app_id};"
    ))
    .unwrap();
    drop(conn);
    (dir, path)
}

/// 테스트가 정한 씨앗으로 도는 작은 난수(선형 합동). 바깥 상자에 기대지 않는다.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}

/// 여러 모양의 줄 하나: (종류, 시각 칸, 내용 글자).
fn random_row(rng: &mut Lcg) -> (&'static str, Option<i64>, String) {
    let at = t(rng.below(10_000) as i64).0;
    let other = at + 1 + rng.below(50) as i64;
    let empty_or = |rng: &mut Lcg, v: i64| if rng.below(2) == 0 { None } else { Some(v) };
    match rng.below(12) {
        // 두 칸 · 시각 칸 비었음
        0 => (
            "feed_status",
            None,
            json!({ "at": at, "payload": { "ok": true } }).to_string(),
        ),
        // 두 칸 · 시각 칸이 내용의 `at`과 같음
        1 => (
            "feed_status",
            Some(at),
            json!({ "at": at, "payload": null }).to_string(),
        ),
        // 두 칸 · 시각 칸이 다름
        2 => (
            "feed_status",
            Some(other),
            json!({ "at": at, "payload": 1 }).to_string(),
        ),
        // 세 칸
        3 => {
            let column = empty_or(rng, at);
            (
                "feed_status",
                column,
                json!({ "at": at, "payload": 1, "extra": true }).to_string(),
            )
        }
        // `at`이 문자열
        4 => {
            let column = empty_or(rng, at);
            (
                "chat",
                column,
                json!({ "at": at.to_string(), "payload": 1 }).to_string(),
            )
        }
        // `at`이 실수
        5 => {
            let column = empty_or(rng, at);
            ("chat", column, format!("{{\"at\":{at}.0,\"payload\":1}}"))
        }
        // JSON 아님
        6 => {
            let column = empty_or(rng, at);
            ("chat", column, "봉투 아님 {".to_string())
        }
        // 배열
        7 => {
            let column = empty_or(rng, at);
            ("chat", column, json!([at, 1]).to_string())
        }
        // `at`만 있음(한 칸)
        8 => ("chat", empty_or(rng, at), json!({ "at": at }).to_string()),
        // 두 칸이지만 `payload`가 없음
        9 => (
            "chat",
            empty_or(rng, at),
            json!({ "at": at, "other": 1 }).to_string(),
        ),
        // 봉투 아닌 객체
        10 => ("chat", empty_or(rng, at), json!({ "n": at }).to_string()),
        // JSON 글자 하나
        _ => ("chat", empty_or(rng, at), json!("글자").to_string()),
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 버전_3_파일의_경계는_접속_관문_4차가_풀_줄_가운데_가장_큰_번호다() {
    for seed in 0..40_u64 {
        let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5);
        let (_dir, path) = empty_v3_file();
        let count = rng.below(13) as i64; // 0~12줄. 0줄인 파일도 나온다.
        let rows: Vec<(i64, &str, Option<i64>, String)> = (1..=count)
            .map(|seq| {
                let (kind, at, payload) = random_row(&mut rng);
                (seq, kind, at, payload)
            })
            .collect();
        raw_insert(&path, &rows);
        assert_eq!(raw_user_version(&path), 3, "씨앗 {seed}");

        let unwrapped: Vec<u64> = raw_events(&path)
            .into_iter()
            .filter(|(_, _, at, payload)| gate_unwraps(*at, payload))
            .map(|(seq, ..)| seq as u64)
            .collect();

        let store = Store::open(&path).unwrap();
        let got = store.legacy_envelope_until().unwrap();
        // 접속 관문 4차가 풀 줄은 모두 경계 안에 든다.
        for seq in &unwrapped {
            assert!(
                got.is_some_and(|until| *seq <= until),
                "씨앗 {seed}: {seq}번이 경계 {got:?} 밖이다. 줄: {rows:?}"
            );
        }
        // 경계는 그런 줄 가운데 가장 큰 번호다(없으면 없음).
        assert_eq!(
            got,
            unwrapped.iter().copied().max(),
            "씨앗 {seed}: 줄: {rows:?}"
        );
    }
}
