#![cfg(test)]
//! 9차: 스키마 버전 5 이전(2026-11-16 조정자 결정 (A)).
//!
//! 옛 파일(버전 1 · 2 · 3 픽스처와, 8차 코드로 만든 버전 4 픽스처 `v4.db`)을 열면 지우지 않고 최신
//! 버전으로 올라가고, 이미 있던 표의 줄은 글자까지 그대로이며, 옛 기록 경계도 그대로다.
//! 새 표(템플릿 · 승인 · 개선안 · 모의계좌 상태 값 · 접속 키)는 비어 있고 바로 쓸 수 있다.
//!
//! `v4.db`: `v3.db`를 8차 코드(버전 4)로 열어 올린 뒤 팀 상태 변경(쉬는 중) · 시각 있는 이벤트 ·
//! 봉투 모양의 시각 없는 이벤트 · 접속 기록을 하나씩 더 쌓은 파일. 경계는 10, 이벤트 13개,
//! 팀 기록 4줄, 접속 기록 3줄, 매매 3건이다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    ApprovalStatus, NewApprovalRequest, NewTemplate, SCHEMA_VERSION, Store, TeamState,
    TemplateState,
};
use rusqlite::types::Value as SqlValue;
use serde_json::json;

const FIXTURES: [&str; 5] = ["v1.db", "v2.db", "v3.db", "v3_plain.db", "v4.db"];

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn copy_of(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(fixture(name), &path).unwrap();
    (dir, path)
}

fn raw_user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn tables(path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// 표들의 모든 줄(칸 값 그대로).
fn dump(path: &Path, names: &[String]) -> Vec<(String, Vec<Vec<SqlValue>>)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    names
        .iter()
        .filter(|name| name.as_str() != "sqlite_sequence")
        .map(|name| {
            let mut stmt = conn
                .prepare(&format!("SELECT * FROM \"{name}\" ORDER BY rowid"))
                .unwrap();
            let width = stmt.column_count();
            let rows = stmt
                .query_map([], |row| {
                    (0..width)
                        .map(|i| row.get::<_, SqlValue>(i))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (name.clone(), rows)
        })
        .collect()
}

fn assert_new_tables_empty_and_usable(store: &Store, label: &str) {
    assert_eq!(store.latest_paper_snapshot().unwrap(), None, "{label}");
    assert_eq!(store.access_key().unwrap(), None, "{label}");
    for state in TemplateState::ALL {
        assert!(
            store.templates_in_state(state).unwrap().is_empty(),
            "{label}"
        );
    }
    for status in ApprovalStatus::ALL {
        assert!(
            store.approvals_in_status(status).unwrap().is_empty(),
            "{label}"
        );
    }
    let template = store
        .create_template(NewTemplate {
            value: json!({ "R%": "1" }),
            at: t(1),
        })
        .unwrap();
    assert_eq!(template.state, TemplateState::Simulating, "{label}");
    let approval = store
        .create_approval(NewApprovalRequest {
            fingerprint: "sha256:x".to_string(),
            summary: json!({}),
            created_at: t(1),
            expires_at: t(2),
        })
        .unwrap();
    assert_eq!(approval.status, ApprovalStatus::Pending, "{label}");
    assert_eq!(
        store.save_paper_snapshot(&json!({ "v": 1 }), t(1)).unwrap(),
        1,
        "{label}"
    );
}

// 검증: D6, V6, M1, M2, M5, M6, C2, E3
#[test]
fn 스키마_버전은_5이고_새_저장소는_5로_만들어진다() {
    assert_eq!(SCHEMA_VERSION, 7);
    let (_dir, path, store) = temp_store();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_new_tables_empty_and_usable(&store, "새 파일");
    drop(store);
    assert_eq!(raw_user_version(&path), 7);
}

// 검증: D6, V6, M1, M2, M5, M6, C2, E3
#[test]
fn 옛_파일을_열면_지우지_않고_5로_올라가고_옛_표가_남는다() {
    for name in FIXTURES {
        let (_dir, path) = copy_of(name);
        let before_tables = tables(&path);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.info().unwrap().schema_version, 7, "{name}");
        drop(store);
        assert_eq!(raw_user_version(&path), 7, "{name}");
        let after_tables = tables(&path);
        for table in &before_tables {
            assert!(after_tables.contains(table), "{name}: {table}");
        }
    }
}

// 검증: D6, V6, M1, M2, M5, M6, C2, E3
#[test]
fn 버전_4_파일을_5로_올려도_있던_줄이_글자까지_그대로다() {
    // 버전 1 ~ 3은 4로 올라가는 단계에서 시각 칸을 채우므로(3차 · 4차) 줄이 그대로인 것은 버전 4
    // 파일로 본다. 버전 1 ~ 3 파일의 자료가 최신 버전에서 그대로인 것은 3차 · 4차 테스트가 본다.
    let (_dir, path) = copy_of("v4.db");
    let names = tables(&path);
    let before = dump(&path, &names);
    drop(Store::open(&path).unwrap());
    assert_eq!(raw_user_version(&path), 7);
    assert_eq!(dump(&path, &names), before);
}

// 검증: D6, C2, E3
#[test]
fn 버전_4_파일의_경계와_자료는_5로_올려도_그대로다() {
    let (_dir, path) = copy_of("v4.db");
    let raw_boundary: Option<i64> = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT until_seq FROM legacy_boundary WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw_boundary, Some(10));
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    let events = store.read_after(0, 100).unwrap();
    assert_eq!(events.len(), 13);
    assert_eq!(events[11].kind, "chat");
    assert_eq!(events[11].payload, json!({ "v4": true }));
    assert_eq!(events[12].kind, "note");
    assert_eq!(
        events[12].payload,
        json!({ "at": 5, "payload": "after boundary" })
    );
    // 경계 뒤의 봉투 모양 줄은 옛 봉투가 아니다.
    assert!(!store.legacy_envelope_seqs(0, 100).unwrap().contains(&13));
    assert_eq!(store.team_state().unwrap(), TeamState::Resting);
    assert_eq!(store.team_history().unwrap().len(), 4);
    assert_eq!(store.access_log().unwrap().len(), 3);
    // 이벤트 번호는 이어진다.
    assert_eq!(store.append("chat", &json!({ "n": 14 })).unwrap(), 14);
}

// 검증: D6, V6, M3, M5, M6
#[test]
fn 올린_옛_파일의_새_표는_비어_있고_바로_쓸_수_있다() {
    for name in FIXTURES {
        let (_dir, path) = copy_of(name);
        let store = Store::open(&path).unwrap();
        assert_new_tables_empty_and_usable(&store, name);
    }
}

// 검증: D6, M3, M5, V6
#[test]
fn 올린_파일을_다시_열어도_5이고_새_표의_자료가_그대로다() {
    let (_dir, path) = copy_of("v4.db");
    let store = Store::open(&path).unwrap();
    let template = store
        .create_template(NewTemplate {
            value: json!({ "R%": "1" }),
            at: t(1),
        })
        .unwrap();
    store.save_paper_snapshot(&json!({ "v": 7 }), t(2)).unwrap();
    drop(store);
    let names = tables(&path);
    let before = dump(&path, &names);
    for _ in 0..2 {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.info().unwrap().schema_version, 7);
        assert_eq!(store.get_template(template.id).unwrap(), template);
        assert_eq!(
            store.latest_paper_snapshot().unwrap().unwrap().state,
            json!({ "v": 7 })
        );
        drop(store);
        // 이전을 되풀이하지 않는다: 어떤 줄도 바뀌지 않았다.
        assert_eq!(dump(&path, &names), before);
    }
}

// 검증: D6, M5, M3
#[test]
fn 올린_파일에서도_새_표의_보호가_선다() {
    for name in FIXTURES {
        let (_dir, path) = copy_of(name);
        let store = Store::open(&path).unwrap();
        let approval = store
            .create_approval(NewApprovalRequest {
                fingerprint: "sha256:x".to_string(),
                summary: json!({}),
                created_at: t(1),
                expires_at: t(100),
            })
            .unwrap();
        let template = store
            .create_template(NewTemplate {
                value: json!({}),
                at: t(1),
            })
            .unwrap();
        drop(store);
        let conn = rusqlite::Connection::open(&path).unwrap();
        assert!(
            conn.execute("UPDATE approvals SET status = 'bogus'", [])
                .is_err(),
            "{name}"
        );
        assert!(conn.execute("DELETE FROM approvals", []).is_err(), "{name}");
        assert!(
            conn.execute("UPDATE templates SET state = 'live'", [])
                .is_err(),
            "{name}"
        );
        drop(conn);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get_approval(approval.id).unwrap(), approval);
        assert_eq!(store.get_template(template.id).unwrap(), template);
    }
}
