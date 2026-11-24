#![cfg(test)]
//! 후보 일괄 처리: 스키마 버전 8 이전 · 표가 스스로 지키는 것 · 잠금을 쥐지 않는 스냅샷.
//!
//! - 버전 7 파일(`fixtures/v7.db`)을 8로 올려도 있던 줄이 글자까지 그대로이고, 새 표 · 새 API를 바로 쓴다.
//! - 지식 조각표: `(kind, record_ref, ordinal)`이 겹치는 대체 넣기도 거부되어 색인 · 임베딩이 고아로 남지 않는다.
//! - 매매 계획 · 상태 이력 표: 어떤 SQL로도 방향을 바꾸거나 잠긴 수량을 고치거나 이력을 지우지 못한다.
//! - 개선안 위험 표시: SQL로도 내리지 못하고, 제안됨이 아니면 올리지도 못한다.
//! - 스냅샷: 뜨는 동안 같은 손잡이의 쓰기가 기다리지 않는다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

use common::{t, temp_store};
use pi_store::{
    ApprovalId, ChunkId, KnowledgeKind, ProposalId, ProposalState, SCHEMA_VERSION, Store,
    StoreError, TradeId, TradeSide, TradeState,
};
use rusqlite::types::Value as SqlValue;

// ───────────── 도우미 ─────────────

fn copy_of_v7() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v7.db"),
        &path,
    )
    .unwrap();
    (dir, path)
}

fn raw(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

fn user_version(path: &Path) -> i64 {
    raw(path)
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn count(path: &Path, sql: &str) -> i64 {
    raw(path).query_row(sql, [], |r| r.get(0)).unwrap()
}

/// 일반 표 이름들. FTS5 보조 표(WITHOUT ROWID)는 rowid가 없어 건너뛴다.
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

fn rows(path: &Path, table: &str) -> Vec<Vec<SqlValue>> {
    let conn = raw(path);
    let mut stmt = conn
        .prepare(&format!("SELECT rowid, * FROM \"{table}\" ORDER BY rowid"))
        .unwrap();
    let width = stmt.column_count();
    stmt.query_map([], |row| {
        (0..width)
            .map(|i| row.get::<_, SqlValue>(i))
            .collect::<Result<Vec<_>, _>>()
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn dump(path: &Path, names: &[String]) -> Vec<(String, Vec<Vec<SqlValue>>)> {
    names
        .iter()
        .map(|name| (name.clone(), rows(path, name)))
        .collect()
}

/// SQL이 거부(제약 오류)되는지.
fn rejected(path: &Path, sql: &str) -> bool {
    raw(path).execute_batch(sql).is_err()
}

// ───────────── 버전 8 이전 ─────────────

// 검증: D6 M2
#[test]
fn 버전_7_파일을_8로_올려도_있던_줄이_글자까지_그대로이고_새_표는_비어_있다() {
    assert_eq!(SCHEMA_VERSION, 11);
    let (_dir, path) = copy_of_v7();
    assert_eq!(user_version(&path), 7);
    let names = tables(&path);
    let before = dump(&path, &names);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 11);
    drop(store);
    assert_eq!(user_version(&path), 11);
    assert_eq!(dump(&path, &names), before);
    for table in ["trade_plans", "trade_state_changes"] {
        assert!(rows(&path, table).is_empty(), "{table}");
    }
    // 검색 색인도 그대로다.
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_fts"), 2);

    // 다시 열어도 아무것도 바뀌지 않는다.
    let all = tables(&path);
    let again = dump(&path, &all);
    drop(Store::open(&path).unwrap());
    assert_eq!(dump(&path, &all), again);
}

// 검증: D6 M2 M4 D10
#[test]
fn 올린_파일의_옛_자료에_새_api를_바로_쓴다() {
    let (_dir, path) = copy_of_v7();
    let store = Store::open(&path).unwrap();
    // 옛 매매: 방향 · 수량 · 이력이 없다.
    for id in 1..=3 {
        assert_eq!(store.trade_side(TradeId(id)).unwrap(), None);
        assert_eq!(store.trade_quantity(TradeId(id)).unwrap(), None);
        assert!(store.trade_state_changes(TradeId(id)).unwrap().is_empty());
    }
    // 승인이 없는 판단된 매매에는 수량을 두고, 승인이 붙은 매매에는 못 둔다.
    store.set_trade_quantity(TradeId(1), 5).unwrap();
    assert_eq!(store.trade_quantity(TradeId(1)).unwrap(), Some(5));
    let err = store.set_trade_quantity(TradeId(2), 5).unwrap_err();
    assert!(matches!(err, StoreError::QuantityLocked { .. }), "{err:?}");
    // 옛 매매의 전이도 이력에 남는다.
    store
        .transition_trade(TradeId(3), TradeState::AwaitingApproval, t(500))
        .unwrap();
    let changes = store.trade_state_changes(TradeId(3)).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].from, Some(TradeState::Judged));
    assert_eq!(changes[0].to, TradeState::AwaitingApproval);
    assert_eq!(changes[0].at, t(500));
    // 옛 개선안에 위험 표시를 올린다.
    let marked = store.mark_proposal_risky(ProposalId(1), t(501)).unwrap();
    assert!(marked.increases_risk);
    assert_eq!(marked.state, ProposalState::Proposed);
    // 옛 조각을 번호로 읽고, 옛 메타 키를 지운다.
    let chunk = store.knowledge_chunk(ChunkId(1)).unwrap().unwrap();
    assert_eq!(chunk.record_ref, "trade-1");
    assert!(store.remove_knowledge_meta("chunk.1").unwrap());
    assert_eq!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "trade-1")
            .unwrap()
            .len(),
        2
    );
    let _ = ApprovalId(0);
}

// ───────────── 표가 스스로 지키는 것 ─────────────

// 검증: D10 M7
#[test]
fn 조각_순서가_겹치는_대체_넣기는_번호가_달라도_거부되고_색인과_임베딩이_그대로다() {
    let (_dir, path) = copy_of_v7();
    drop(Store::open(&path).unwrap());
    let names = tables(&path);
    let before = dump(&path, &names);
    let fts = count(&path, "SELECT count(*) FROM knowledge_fts");
    for verb in ["INSERT OR REPLACE INTO", "REPLACE INTO"] {
        for (id, ordinal) in [(100, 0), (101, 1)] {
            let sql = format!(
                "{verb} knowledge_chunks (id, kind, source, record_ref, ordinal, text, updated_at)
                 VALUES ({id}, 'trade', 'mine', 'trade-1', {ordinal}, '바꿔치기', 0)"
            );
            assert!(rejected(&path, &sql), "{sql}");
        }
    }
    assert_eq!(dump(&path, &names), before);
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_fts"), fts);
    // 겹치지 않는 넣기는 된다(트리거가 지나치게 막지 않는다).
    assert!(!rejected(
        &path,
        "INSERT INTO knowledge_chunks (kind, source, record_ref, ordinal, text, updated_at)
         VALUES ('trade', 'mine', 'trade-1', 2, '셋째 조각', 0)"
    ));
}

// 검증: M2
#[test]
fn 매매_계획_표는_sql로도_방향을_바꾸거나_잠긴_수량을_고치거나_지울_수_없다() {
    let (_dir, path, store) = temp_store();
    let open = store
        .create_trade(pi_store::NewTrade {
            symbol: "005930".to_string(),
            account: pi_store::AccountKind::Paper,
            at: t(1),
        })
        .unwrap()
        .id;
    store.set_trade_quantity(open, 3).unwrap();
    store
        .judge_trade_with_side(open, judgment(10), TradeSide::Sell)
        .unwrap();
    store
        .transition_trade(open, TradeState::AwaitingApproval, t(20))
        .unwrap();
    store
        .set_trade_approval(
            open,
            pi_store::Approval {
                number: 1,
                content_fingerprint: "sha256:o".to_string(),
            },
            t(21),
        )
        .unwrap();
    drop(store);
    let before = rows(&path, "trade_plans");
    let id = open.0;
    for sql in [
        format!("UPDATE trade_plans SET side = 'buy' WHERE trade_id = {id}"),
        format!("UPDATE trade_plans SET side = NULL WHERE trade_id = {id}"),
        format!("UPDATE trade_plans SET quantity = 30 WHERE trade_id = {id}"),
        format!("UPDATE trade_plans SET quantity = NULL WHERE trade_id = {id}"),
        format!("UPDATE trade_plans SET trade_id = 99 WHERE trade_id = {id}"),
        format!("DELETE FROM trade_plans WHERE trade_id = {id}"),
        format!(
            "INSERT OR REPLACE INTO trade_plans (trade_id, side, quantity) VALUES ({id}, 'buy', 1)"
        ),
        format!("REPLACE INTO trade_plans (rowid, side, quantity) VALUES ({id}, 'buy', 1)"),
        format!("UPDATE trade_plans SET quantity = 0 WHERE trade_id = {id}"),
    ] {
        assert!(rejected(&path, &sql), "{sql}");
    }
    assert_eq!(rows(&path, "trade_plans"), before);
}

// 검증: M2
#[test]
fn 상태_이력_표는_쌓기만_하고_sql로_바꾼_상태도_남는다() {
    let (_dir, path, store) = temp_store();
    let trade = store
        .create_trade(pi_store::NewTrade {
            symbol: "005930".to_string(),
            account: pi_store::AccountKind::Paper,
            at: t(1),
        })
        .unwrap()
        .id;
    store.judge_trade(trade, judgment(10)).unwrap();
    drop(store);
    let before = rows(&path, "trade_state_changes");
    assert_eq!(before.len(), 2);
    for sql in [
        "UPDATE trade_state_changes SET to_state = 'sold'",
        "UPDATE trade_state_changes SET at = 0",
        "DELETE FROM trade_state_changes",
        "INSERT OR REPLACE INTO trade_state_changes (seq, trade_id, from_state, to_state, at)
         VALUES (1, 1, NULL, 'holding', 0)",
        "REPLACE INTO trade_state_changes (rowid, trade_id, from_state, to_state, at)
         VALUES (2, 1, 'judged', 'holding', 0)",
    ] {
        assert!(rejected(&path, sql), "{sql}");
    }
    assert_eq!(rows(&path, "trade_state_changes"), before);

    // 공개 API를 거치지 않은 상태 바꾸기도 이력에 남는다.
    raw(&path)
        .execute_batch(&format!(
            "UPDATE trades SET state = 'awaiting_approval', updated_at = {} WHERE id = {}",
            t(30).0,
            trade.0
        ))
        .unwrap();
    let store = Store::open(&path).unwrap();
    let changes = store.trade_state_changes(trade).unwrap();
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[2].from, Some(TradeState::Judged));
    assert_eq!(changes[2].to, TradeState::AwaitingApproval);
    assert_eq!(changes[2].at, t(30));
}

// 검증: M4
#[test]
fn 위험_표시는_sql로도_내리지_못하고_제안됨이_아니면_올리지_못한다() {
    let (_dir, path) = copy_of_v7();
    let store = Store::open(&path).unwrap();
    let a = store
        .create_proposal(pi_store::NewProposal {
            content: serde_json::json!({ "위험": true }),
            evidence: vec![TradeId(1), TradeId(2)],
            increases_risk: true,
            at: t(600),
        })
        .unwrap();
    let b = store
        .create_proposal(pi_store::NewProposal {
            content: serde_json::json!({ "위험": false }),
            evidence: vec![TradeId(1), TradeId(3)],
            increases_risk: false,
            at: t(601),
        })
        .unwrap();
    store.reject_proposal(b.id, t(602)).unwrap();
    drop(store);
    let before = rows(&path, "proposals");
    for sql in [
        format!(
            "UPDATE proposals SET increases_risk = 0 WHERE id = {}",
            a.id.0
        ),
        format!(
            "UPDATE proposals SET increases_risk = 1 WHERE id = {}",
            b.id.0
        ),
    ] {
        assert!(rejected(&path, &sql), "{sql}");
    }
    assert_eq!(rows(&path, "proposals"), before);
    // 제안됨인 옛 개선안은 SQL로도 올릴 수 있다(내리지는 못한다).
    assert!(!rejected(
        &path,
        "UPDATE proposals SET increases_risk = 1 WHERE id = 1"
    ));
    assert!(rejected(
        &path,
        "UPDATE proposals SET increases_risk = 0 WHERE id = 1"
    ));
}

// ───────────── 스냅샷이 쓰기를 막지 않는다 ─────────────

// 검증: D10
#[test]
fn 스냅샷을_뜨는_동안_같은_손잡이의_쓰기가_기다리지_않는다() {
    let (_dir, path, store) = temp_store();
    // 뜨는 데 시간이 걸리게 파일을 키운다(약 48MB).
    raw(&path)
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 24000)
             INSERT INTO knowledge_meta (key, value) SELECT 'big.' || i, hex(randomblob(1000)) FROM n;",
        )
        .unwrap();
    let copy_dir = tempfile::tempdir().unwrap();
    let copy = copy_dir.path().join("snap.sqlite");
    let snapper = store.clone();
    let target = copy.clone();
    let started = Instant::now();
    let handle = thread::spawn(move || snapper.snapshot_to(&target));
    while !copy.exists() && !handle.is_finished() {
        thread::yield_now();
    }
    let mut during = 0;
    while !handle.is_finished() {
        store
            .append("chat", &serde_json::json!({ "n": during }))
            .unwrap();
        if !handle.is_finished() {
            during += 1;
        }
    }
    handle.join().unwrap().unwrap();
    let took = started.elapsed();
    assert!(
        during >= 3,
        "스냅샷({took:?}) 동안 끝난 쓰기가 {during}번뿐이다(잠금을 쥐고 있다)"
    );
    let snap = Store::open(&copy).unwrap();
    assert_eq!(snap.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_eq!(count(&copy, "SELECT count(*) FROM knowledge_meta"), 24_000);
}

fn judgment(at: i64) -> pi_store::Judgment {
    pi_store::Judgment {
        decided_at: t(at),
        rationale: pi_store::Rationale::default(),
        sources: Vec::new(),
        scores: Vec::new(),
        consensus_score: 1,
    }
}
