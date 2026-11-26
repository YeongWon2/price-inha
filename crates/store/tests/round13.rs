#![cfg(test)]
//! 13차: 스키마 버전 9 이전 · 표가 스스로 지키는 것.
//!
//! - 버전 8 파일(`fixtures/v8.db`)을 9로 올려도 있던 줄이 글자까지 그대로이고 새 보호가 바로 선다.
//! - 지식 조각: 갱신으로 다른 줄과 겹치게 바꿔 줄을 없애는 길(`UPDATE OR REPLACE`)을 막는다.
//! - 매매 방향: NULL에서 값으로 정하는 것은 논의중 · 판단됨일 때만 된다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use pi_store::{SCHEMA_VERSION, Store};
use rusqlite::types::Value as SqlValue;

fn copy_of_v8() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v8.db"),
        &path,
    )
    .unwrap();
    (dir, path)
}

fn raw(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

fn count(path: &Path, sql: &str) -> i64 {
    raw(path).query_row(sql, [], |r| r.get(0)).unwrap()
}

fn tables(path: &Path) -> Vec<String> {
    raw(path)
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table'
             AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'knowledge_fts%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn dump(path: &Path, names: &[String]) -> Vec<(String, Vec<Vec<SqlValue>>)> {
    let conn = raw(path);
    names
        .iter()
        .map(|name| {
            let mut stmt = conn
                .prepare(&format!("SELECT rowid, * FROM \"{name}\" ORDER BY rowid"))
                .unwrap();
            let width = stmt.column_count();
            let rows = stmt
                .query_map([], |row| {
                    (0..width)
                        .map(|i| row.get::<_, SqlValue>(i))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (name.clone(), rows)
        })
        .collect()
}

fn rejected(path: &Path, sql: &str) -> bool {
    raw(path).execute_batch(sql).is_err()
}

// 검증: D6 M2
#[test]
fn 버전_8_파일을_9로_올려도_있던_줄이_글자까지_그대로다() {
    assert_eq!(SCHEMA_VERSION, 12);
    let (_dir, path) = copy_of_v8();
    assert_eq!(count(&path, "PRAGMA user_version"), 8);
    let names = tables(&path);
    let before = dump(&path, &names);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 12);
    drop(store);
    assert_eq!(count(&path, "PRAGMA user_version"), 12);
    assert_eq!(dump(&path, &names), before);
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_fts"), 2);

    // 다시 열어도 아무것도 바뀌지 않는다.
    let all = tables(&path);
    let again = dump(&path, &all);
    drop(Store::open(&path).unwrap());
    assert_eq!(dump(&path, &all), again);
}

// 검증: D10 M7
#[test]
fn 조각을_갱신으로_다른_줄과_겹치게_바꾸는_것은_거부되고_색인이_그대로다() {
    let (_dir, path) = copy_of_v8();
    drop(Store::open(&path).unwrap());
    let names = tables(&path);
    let before = dump(&path, &names);
    let fts = count(&path, "SELECT count(*) FROM knowledge_fts");
    let kept = count(&path, "SELECT count(*) FROM knowledge_chunks");
    for sql in [
        "UPDATE OR REPLACE knowledge_chunks SET ordinal = 0 WHERE id = 2",
        "UPDATE knowledge_chunks SET ordinal = 0 WHERE id = 2",
        "UPDATE OR REPLACE knowledge_chunks SET id = 1 WHERE id = 2",
        "UPDATE OR REPLACE knowledge_chunks SET ordinal = ordinal + 1",
    ] {
        assert!(rejected(&path, sql), "{sql}");
    }
    assert_eq!(dump(&path, &names), before);
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_fts"), fts);
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_chunks"), kept);
    // 겹치지 않는 갱신은 된다.
    assert!(!rejected(
        &path,
        "UPDATE knowledge_chunks SET text = '고친 글' WHERE id = 1"
    ));
    assert!(!rejected(
        &path,
        "UPDATE knowledge_chunks SET ordinal = 7 WHERE id = 2"
    ));
}

// 검증: M2
#[test]
fn 매매_방향은_열린_상태에서만_처음_정하고_정한_뒤에는_그대로다() {
    let (_dir, path) = copy_of_v8();
    drop(Store::open(&path).unwrap());
    // 매매 2는 승인대기, 매매 1 · 3은 판단됨(3은 이미 방향이 있다).
    assert!(rejected(
        &path,
        "UPDATE trade_plans SET side = 'buy' WHERE trade_id = 2"
    ));
    assert!(rejected(
        &path,
        "UPDATE OR REPLACE trade_plans SET side = 'sell' WHERE trade_id = 2"
    ));
    assert!(rejected(
        &path,
        "INSERT INTO trades (symbol, account, state, created_at, updated_at)
         VALUES ('Z', 'paper', 'executed', 1, 1);
         INSERT INTO trade_plans (trade_id, side) VALUES (4, 'buy');"
    ));
    assert_eq!(
        count(
            &path,
            "SELECT count(*) FROM trade_plans WHERE side IS NOT NULL"
        ),
        1
    );
    // 판단됨에서는 처음 정할 수 있고, 정한 뒤에는 바꿀 수 없다.
    assert!(!rejected(
        &path,
        "UPDATE trade_plans SET side = 'buy' WHERE trade_id = 1"
    ));
    assert!(rejected(
        &path,
        "UPDATE trade_plans SET side = 'sell' WHERE trade_id = 1"
    ));
    assert!(rejected(
        &path,
        "UPDATE trade_plans SET side = 'buy' WHERE trade_id = 3"
    ));
}

// 검증: M2
#[test]
fn 방향을_정하며_판단하는_길은_그대로_된다() {
    let (_dir, _path, store) = common::temp_store();
    let id = store
        .create_trade(pi_store::NewTrade {
            symbol: "005930".to_string(),
            account: pi_store::AccountKind::Paper,
            at: common::t(1),
        })
        .unwrap()
        .id;
    store.set_trade_quantity(id, 3).unwrap();
    assert_eq!(store.trade_side(id).unwrap(), None);
}
