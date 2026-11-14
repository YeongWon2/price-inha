#![cfg(test)]
//! 4차 4번: 기록 표를 바꾸는 우회 SQL의 빈칸.
//!
//! 2 · 3차 테스트는 이벤트 표의 `UPDATE OR REPLACE`와 이벤트 · 접속 기록의 `ON CONFLICT DO UPDATE` 몇 줄만
//! 넣어 봤다. 여기서는 기록 표 넷(투자 팀 기록 · 이벤트 · 접속 기록 · 승인 번호 목록)의 모든 칸에
//! `UPDATE OR <충돌 처리>` 다섯 가지와 `INSERT … ON CONFLICT DO UPDATE`(대상 칸을 주거나 안 주거나),
//! `REPLACE INTO`를 넣어 보고, 공개 API로 다시 읽어 그대로인지 본다.
//! 새로 만든 파일, 버전 2에서 올린 파일, 버전 3에서 올린 파일(4차 픽스처) 모두에서 본다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    AccessRecord, AccountKind, Approval, EventRange, NewAccess, NewTrade, Store, StoreError,
    TeamCause, TeamChange, TeamState, TeamStateRecord, TimedEvent, TradeId,
};
use serde_json::json;

const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");

/// 기록 표와 그 줄을 가리키는 칸.
const RECORD_TABLES: [(&str, &str); 4] = [
    ("team_history", "seq"),
    ("events", "seq"),
    ("access_log", "seq"),
    ("approval_numbers", "number"),
];

fn copy_of(fixture: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(fixture, &path).unwrap();
    (dir, path)
}

#[derive(Debug, PartialEq)]
struct Records {
    team_state: TeamState,
    team_history: Vec<TeamStateRecord>,
    events: Vec<TimedEvent>,
    access_log: Vec<AccessRecord>,
}

fn read_records(path: &Path) -> Records {
    let store = Store::open(path).unwrap();
    Records {
        team_state: store.team_state().unwrap(),
        team_history: store.team_history().unwrap(),
        events: store.read_events(EventRange::default(), 1_000).unwrap(),
        access_log: store.access_log().unwrap(),
    }
}

/// 이미 쓴 승인 번호는 아직 승인이 없는 매매(`free`)도 쓸 수 없다.
fn assert_numbers_used(path: &Path, free: TradeId, numbers: &[i64]) {
    let store = Store::open(path).unwrap();
    for &number in numbers {
        match store.set_trade_approval(
            free,
            Approval {
                number,
                content_fingerprint: "sha256:reuse".to_string(),
            },
            t(9_000),
        ) {
            Err(StoreError::ApprovalNumberReused { number: n }) => assert_eq!(n, number),
            other => panic!("{number}번은 이미 쓴 승인 번호다: {other:?}"),
        }
    }
}

fn columns(path: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// SQL을 파일에 직접 넣어 본다. 막혀서 거절되거나(제약 오류) 조용히 무시되는 것은 괜찮다.
/// 그 밖의 오류는 시험하는 SQL 자체가 틀린 것이므로 멈춘다.
fn try_sql(path: &Path, sql: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    match conn.execute(sql, []) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation => {}
        Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
    }
}

/// 기록 표마다 우회 SQL을 만든다. 표마다 번호(`key`)가 1 · 2인 줄이 있어야 한다.
fn bypass_sqls(path: &Path) -> Vec<String> {
    let mut sqls = Vec::new();
    for (table, key) in RECORD_TABLES {
        let cols = columns(path, table);
        assert!(cols.iter().any(|c| c == key), "{table}");
        let list = cols.join(", ");
        // 번호 칸만 1로 바꾼 2번 줄의 값들.
        let shifted = cols
            .iter()
            .map(|c| if c == key { "1".to_string() } else { c.clone() })
            .collect::<Vec<_>>()
            .join(", ");
        for column in &cols {
            for conflict in ["REPLACE", "IGNORE", "FAIL", "ABORT", "ROLLBACK"] {
                sqls.push(format!(
                    "UPDATE OR {conflict} {table} SET {column} = \
                     (SELECT {column} FROM {table} WHERE {key} = 1) WHERE {key} = 2"
                ));
            }
            sqls.push(format!(
                "UPDATE OR REPLACE {table} SET {column} = 12345 WHERE {key} = 2"
            ));
            // 같은 번호의 줄을 넣으며 부딪히면 고치게 한다(대상 칸을 주고 · 안 주고).
            sqls.push(format!(
                "INSERT INTO {table} ({list}) SELECT {shifted} FROM {table} WHERE {key} = 2
                 ON CONFLICT ({key}) DO UPDATE SET {column} = excluded.{column}"
            ));
            sqls.push(format!(
                "INSERT INTO {table} ({list}) SELECT {list} FROM {table} WHERE true
                 ON CONFLICT DO UPDATE SET {column} = 12345"
            ));
            sqls.push(format!(
                "INSERT INTO {table} ({list}) SELECT {list} FROM {table} WHERE true
                 ON CONFLICT ({key}) DO NOTHING ON CONFLICT DO UPDATE SET {column} = 12345"
            ));
        }
        sqls.push(format!(
            "REPLACE INTO {table} ({list}) SELECT {shifted} FROM {table} WHERE {key} = 2"
        ));
        sqls.push(format!(
            "INSERT OR REPLACE INTO {table} ({list}) SELECT {list} FROM {table}"
        ));
    }
    sqls
}

fn assert_bypass_blocked(path: &Path, free: TradeId, numbers: &[i64]) {
    let before = read_records(path);
    assert!(before.team_history.len() >= 2);
    assert!(before.events.len() >= 2);
    assert!(before.access_log.len() >= 2);
    assert!(numbers.len() >= 2);
    assert_numbers_used(path, free, numbers);

    for sql in bypass_sqls(path) {
        try_sql(path, &sql);
        assert_eq!(read_records(path), before, "바뀌었다: {sql}");
    }
    assert_numbers_used(path, free, numbers);
    // 번호가 1 · 2인 승인 번호 줄의 주인도 그대로다(다른 매매로 옮겨지지 않았다).
    let conn = rusqlite::Connection::open(path).unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM approval_numbers", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows as usize, numbers.len());
}

// 검증: M1, C2, E3, M6, M5
#[test]
fn 새로_만든_파일의_기록_표는_update_or_와_on_conflict_do_update로도_바뀌지_않는다() {
    let (_dir, path, store) = temp_store();
    for (i, (to, cause)) in [
        (TeamState::Operating, TeamCause::TradingStart),
        (TeamState::Resting, TeamCause::MarketClose),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .change_team_state(TeamChange {
                to,
                at: t(i as i64),
                cause,
                detail: format!("{i}번째 변경"),
                owner_approved: false,
            })
            .unwrap();
    }
    for n in 1..=2_i64 {
        store
            .append_at("chat", &json!({ "n": n }), t(10 + n))
            .unwrap();
        store
            .record_access(NewAccess {
                at: t(100 + n),
                origin: "127.0.0.1".to_string(),
                action: format!("{n}번째 접속"),
            })
            .unwrap();
    }
    // 승인 번호 1 · 2를 두 매매에 붙이고, 셋째 매매는 승인 없이 둔다.
    let mut ids = Vec::new();
    for n in 1..=3_i64 {
        let trade = store
            .create_trade(NewTrade {
                symbol: format!("00593{n}"),
                account: AccountKind::Paper,
                at: t(200 + n),
            })
            .unwrap();
        ids.push(trade.id);
    }
    for (i, number) in [1_i64, 2].into_iter().enumerate() {
        store
            .set_trade_approval(
                ids[i],
                Approval {
                    number,
                    content_fingerprint: format!("sha256:{number}"),
                },
                t(300),
            )
            .unwrap();
    }
    drop(store);
    assert_bypass_blocked(&path, ids[2], &[1, 2]);
}

/// 버전 2 · 3 픽스처에는 승인 번호 41 · 77만 있다. 번호 1 · 2인 줄을 만들려고 매매 1(논의중)에
/// 1번을 붙이고, 새 매매에 2번을 붙인다. 승인 없는 매매를 하나 더 만들어 돌려준다.
fn add_numbers_one_two(path: &Path) -> TradeId {
    let store = Store::open(path).unwrap();
    store
        .set_trade_approval(
            TradeId(1),
            Approval {
                number: 1,
                content_fingerprint: "sha256:1".to_string(),
            },
            t(8_000),
        )
        .unwrap();
    let new_trade = || {
        store
            .create_trade(NewTrade {
                symbol: "000660".to_string(),
                account: AccountKind::Live,
                at: t(8_001),
            })
            .unwrap()
            .id
    };
    let second = new_trade();
    let free = new_trade();
    store
        .set_trade_approval(
            second,
            Approval {
                number: 2,
                content_fingerprint: "sha256:2".to_string(),
            },
            t(8_002),
        )
        .unwrap();
    free
}

// 검증: D6, V6, M1, C2, E3, M6, M5
#[test]
fn 버전_2에서_올린_파일의_기록_표도_update_or_와_on_conflict_do_update로_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let free = add_numbers_one_two(&path);
    assert_bypass_blocked(&path, free, &[1, 2, 41, 77]);
}

// 검증: D6, V6, M1, C2, E3, M6, M5
#[test]
fn 버전_3에서_올린_파일의_기록_표도_update_or_와_on_conflict_do_update로_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let free = add_numbers_one_two(&path);
    assert_eq!(
        Store::open(&path).unwrap().info().unwrap().schema_version,
        4
    );
    assert_bypass_blocked(&path, free, &[1, 2, 41, 77]);
}
