#![cfg(test)]
//! 10차 보강: `INSERT OR REPLACE` · `REPLACE INTO`로 보호하는 표의 줄을 바꿔치기하는 우회.
//!
//! SQLite는 `recursive_triggers`가 꺼져 있어, 키가 겹친 줄을 `REPLACE`가 지울 때 지우기 막기
//! 트리거가 돌지 않는다. 그래서 보호한다고 적은 모든 표(앞 차수 것 포함)는 키가 겹치는 넣기를
//! 스스로 거부해야 한다. 여기서는
//! 1. 공통 우회: 보호하는 표마다 모든 줄을 그대로 다시 넣어 본다(기존 줄 키).
//! 2. 키별 우회: room_events `(room, event_id)` · `(room, seq)`, trigger_receipts `(job, slot)`과
//!    같은 job의 running, agent_sessions id와 열린 `(agent, title)`, trades id(판단 시각 V36).
//!
//! 둘 다 거부(제약 오류)되고, 파일의 줄과 공개 API로 읽은 값이 그대로인지 본다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    AccountKind, FactValue, Judgment, MessageRole, NewFact, NewMessage, NewProposal, NewRoomEvent,
    NewTrade, Rationale, RoomActorKind, Store, TradeId, TriggerReceiptKey, TriggerReceiptStatus,
};
use rusqlite::types::Value as SqlValue;
use serde_json::json;

// ───────────── 도우미 ─────────────

/// 보호한다고 적은 표. 지우기를 막는 표는 모두 여기 있어야 한다(아래 테스트가 스키마에서 확인한다).
const PROTECTED: [&str; 20] = [
    "team_history",
    "events",
    "access_log",
    "approval_numbers",
    "legacy_boundary",
    "trades",
    "approvals",
    "templates",
    "template_revisions",
    "proposals",
    "proposal_evidence",
    "paper_snapshots",
    "key_hashes",
    "agent_sessions",
    "agent_messages",
    "room_event_kinds",
    "room_events",
    "trigger_receipts",
    "facts",
    "trade_facts",
];

const VERBS: [&str; 2] = ["INSERT OR REPLACE INTO", "REPLACE INTO"];

const AGENT: &str = "시세 담당";
const OPEN_TITLE: &str = "팀 논의";
const ROOM: &str = "팀";
const CLOSED_JOB: &str = "닫힌 작업";
const RUNNING_JOB: &str = "작업";

fn copy_of_v5() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v5.db"),
        &path,
    )
    .unwrap();
    (dir, path)
}

fn new_trade(store: &Store) -> TradeId {
    store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap()
        .id
}

fn message(content: &str, at: i64) -> NewMessage {
    NewMessage {
        role: MessageRole::User,
        content: Some(content.to_string()),
        api_content: None,
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
        finish_reason: None,
        reasoning: None,
        at: t(at),
    }
}

fn key(job: &str, slot: i64) -> TriggerReceiptKey {
    TriggerReceiptKey {
        job: job.to_string(),
        slot: t(slot),
    }
}

fn fact(known_at: i64) -> NewFact {
    NewFact {
        kind: "financials".to_string(),
        subject: "005930".to_string(),
        label: "매출액".to_string(),
        period: Some("2026-09-30".to_string()),
        value: FactValue::Number("1".to_string()),
        unit: Some("KRW".to_string()),
        reference: None,
        source_name: "dart".to_string(),
        source_url: "http://127.0.0.1:9/fact".to_string(),
        published_on: None,
        known_at: t(known_at),
    }
}

/// 10차 표와 개선안 표에 줄을 쌓는다. 자료가 붙은 판단된 매매를 돌려준다.
///
/// - 세션: 끝난 것 하나와 열린 것(`AGENT`, `OPEN_TITLE`) 하나, 각각 메시지 하나.
/// - 방 `ROOM`: 이벤트 1 · 2번.
/// - 영수증: `CLOSED_JOB` 1번 슬롯은 닫힘(ok), `RUNNING_JOB` 2번 슬롯은 실행 중.
/// - 자료 둘(알게 된 시각 40 · 41)을 판단 시각 50인 매매에 붙인다.
fn fill(store: &Store) -> TradeId {
    let evidence = vec![new_trade(store), new_trade(store)];
    store
        .create_proposal(NewProposal {
            content: json!({ "규칙": "갭 상승 첫 5분에는 사지 않는다" }),
            evidence,
            increases_risk: false,
            at: t(10),
        })
        .unwrap();

    let ended = store.open_session(AGENT, "지난 논의", t(20)).unwrap();
    store
        .append_messages(ended.id, &[message("지난 이야기", 21)])
        .unwrap();
    store.end_session(ended.id, "끝", t(22)).unwrap();
    let open = store.open_session(AGENT, OPEN_TITLE, t(23)).unwrap();
    store
        .append_messages(open.id, &[message("지금 이야기", 24)])
        .unwrap();

    for (i, event_id) in ["e1", "e2"].into_iter().enumerate() {
        store
            .append_room_event(NewRoomEvent {
                room: ROOM.to_string(),
                event_id: event_id.to_string(),
                kind: "message.user".to_string(),
                actor_kind: RoomActorKind::User,
                actor_id: "user-1".to_string(),
                payload: json!({ "text": event_id }),
                at: t(25 + i as i64),
            })
            .unwrap();
    }

    store
        .open_trigger_receipt(&key(CLOSED_JOB, 1), t(30))
        .unwrap();
    store
        .close_trigger_receipt(&key(CLOSED_JOB, 1), TriggerReceiptStatus::Ok, t(31))
        .unwrap();
    store
        .open_trigger_receipt(&key(RUNNING_JOB, 2), t(32))
        .unwrap();

    let facts = store.save_facts(&[fact(40), fact(41)]).unwrap();
    let judged = new_trade(store);
    store
        .judge_trade(
            judged,
            Judgment {
                decided_at: t(50),
                rationale: Rationale::default(),
                sources: Vec::new(),
                scores: Vec::new(),
                consensus_score: 70,
            },
        )
        .unwrap();
    store
        .attach_trade_facts(judged, &[facts[0].id, facts[1].id])
        .unwrap();
    judged
}

/// 새 파일(버전 6)에 `fill`한 것.
fn filled_new() -> (tempfile::TempDir, PathBuf, TradeId) {
    let (dir, path, store) = temp_store();
    let judged = fill(&store);
    drop(store);
    (dir, path, judged)
}

/// 9차 픽스처(버전 5)를 올린 뒤 `fill`한 것. 보호하는 표가 모두 줄을 가진다.
fn filled_v5() -> (tempfile::TempDir, PathBuf, TradeId) {
    let (dir, path) = copy_of_v5();
    let store = Store::open(&path).unwrap();
    let judged = fill(&store);
    drop(store);
    (dir, path, judged)
}

fn columns(path: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let cols: Vec<String> = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!cols.is_empty(), "{table} 표가 없다");
    cols
}

/// 표의 모든 줄(rowid와 칸 값 그대로).
fn rows(path: &Path, table: &str) -> Vec<Vec<SqlValue>> {
    rows_in(&rusqlite::Connection::open(path).unwrap(), table)
}

fn rows_in(conn: &rusqlite::Connection, table: &str) -> Vec<Vec<SqlValue>> {
    let mut stmt = conn
        .prepare(&format!("SELECT rowid, * FROM \"{table}\" ORDER BY rowid"))
        .unwrap();
    let width = stmt.column_count();
    stmt.query_map([], |row| {
        (0..width)
            .map(|i| row.get::<_, SqlValue>(i))
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn rowids(path: &Path, table: &str) -> Vec<i64> {
    rows(path, table)
        .into_iter()
        .map(|row| match row[0] {
            SqlValue::Integer(id) => id,
            ref other => panic!("{table} rowid: {other:?}"),
        })
        .collect()
}

/// 공개 API로 읽은 보호하는 값들.
fn api_view(path: &Path, judged: TradeId) -> String {
    let store = Store::open(path).unwrap();
    format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        store.room_events(ROOM, 0, 100).unwrap(),
        store.trigger_receipts().unwrap(),
        store.sessions_of(AGENT).unwrap(),
        store.get_trade(judged).unwrap(),
        store.trade_facts(judged).unwrap(),
    )
}

/// `table`에서 `filter`에 맞는 줄을 (`overrides`로 몇 칸만 바꿔) 다시 넣는 SQL.
fn replace_sql(
    path: &Path,
    verb: &str,
    table: &str,
    filter: &str,
    overrides: &[(&str, &str)],
) -> String {
    let cols = columns(path, table);
    for (name, _) in overrides {
        assert!(cols.iter().any(|c| c == name), "{table}.{name}");
    }
    let values = cols
        .iter()
        .map(|c| {
            overrides
                .iter()
                .find(|(name, _)| name == c)
                .map_or_else(|| c.clone(), |(_, value)| (*value).to_string())
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{verb} \"{table}\" ({}) SELECT {values} FROM \"{table}\" WHERE {filter}",
        cols.join(", ")
    )
}

/// 우회하는 쪽의 연결. `sqlite3` 명령줄처럼 외래 키 검사를 끈다(표 스스로 막아야 하므로
/// 외래 키에 기대지 않는다).
fn raw(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn
}

/// 우회 SQL을 넣어 본다. 제약 오류로 거부되면 `true`, 들어가 버리면 `false`.
/// 그 밖의 오류나 아무 줄도 고르지 못한 SQL은 시험하는 SQL이 틀린 것이므로 멈춘다.
fn rejected(conn: &rusqlite::Connection, sql: &str) -> bool {
    match conn.execute(sql, []) {
        Ok(changed) => {
            assert!(changed > 0, "아무 줄도 고르지 못한 SQL이다: {sql}");
            false
        }
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            true
        }
        Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
    }
}

/// 한 트랜잭션 안에서 우회 SQL을 넣어 보고 되돌린다. 거부되고 표가 그대로면 `None`,
/// 아니면 무엇이 들어갔는지 돌려준다. 앞 시도가 뒤 시도의 대상 줄을 바꾸지 않게 한다.
fn attempt_in_tx(path: &Path, table: &str, sql: &str) -> Option<String> {
    let conn = raw(path);
    conn.execute_batch("BEGIN").unwrap();
    let before = rows_in(&conn, table);
    let was_rejected = rejected(&conn, sql);
    let unchanged = rows_in(&conn, table) == before;
    conn.execute_batch("ROLLBACK").unwrap();
    match (was_rejected, unchanged) {
        (true, true) => None,
        (false, _) => Some(format!("들어갔다: {sql}")),
        (true, false) => Some(format!("거부됐는데 줄이 바뀌었다: {sql}")),
    }
}

/// 새 파일에 `fill`한 상태에서 두 가지 바꿔치기(`INSERT OR REPLACE` · `REPLACE INTO`)를
/// 하나씩(매번 새 파일로) 넣어 보고, 둘 다 거부되며 표의 줄과 공개 API로 읽은 값이
/// 그대로인지 본다.
fn assert_replace_rejected(table: &str, filter: &str, overrides: &[(&str, &str)]) {
    let mut passed = Vec::new();
    for verb in VERBS {
        let (_dir, path, judged) = filled_new();
        let before_rows = rows(&path, table);
        let before_api = api_view(&path, judged);
        let sql = replace_sql(&path, verb, table, filter, overrides);
        let conn = raw(&path);
        if !rejected(&conn, &sql) {
            passed.push(sql.clone());
        }
        drop(conn);
        if rows(&path, table) != before_rows || api_view(&path, judged) != before_api {
            passed.push(format!("줄이 바뀌었다: {sql}"));
        }
    }
    assert!(
        passed.is_empty(),
        "바꿔치기가 들어갔다 ({table}: {filter}):\n{}",
        passed.join("\n")
    );
}

// ───────────── 공통 우회 ─────────────

// 검증: M1 M2 C11 V36
#[test]
fn 지우기를_막는_표는_모두_보호하는_표_목록에_있다() {
    let (_dir, path, _judged) = filled_v5();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let guarded: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT tbl_name FROM sqlite_master \
             WHERE type = 'trigger' AND upper(sql) LIKE '%BEFORE DELETE ON%' ORDER BY tbl_name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for table in &guarded {
        assert!(
            PROTECTED.contains(&table.as_str()),
            "{table}은 지우기를 막는데 보호하는 표 목록에 없다"
        );
    }
    // 목록의 표는 모두 있고, 이 파일에서 줄을 가진다(아래 공통 우회가 빈 표를 건너뛰지 않게).
    for table in PROTECTED {
        columns(&path, table);
        assert!(!rows(&path, table).is_empty(), "{table}에 줄이 없다");
    }
}

// 검증: M1 M2 C11 V36
#[test]
fn 보호하는_모든_표에_기존_줄을_그대로_다시_넣으면_거부되고_줄이_그대로다() {
    let (_dir, path, judged) = filled_v5();
    let before_api = api_view(&path, judged);
    let mut passed = Vec::new();
    for table in PROTECTED {
        let before = rows(&path, table);
        for rowid in rowids(&path, table) {
            for verb in VERBS {
                let sql = replace_sql(&path, verb, table, &format!("rowid = {rowid}"), &[]);
                passed.extend(attempt_in_tx(&path, table, &sql));
            }
        }
        if rows(&path, table) != before {
            passed.push(format!("{table}: 줄이 바뀌었다"));
        }
    }
    assert!(
        passed.is_empty(),
        "키가 겹치는 넣기가 들어갔다:\n{}",
        passed.join("\n")
    );
    assert_eq!(api_view(&path, judged), before_api);
}

// 검증: M1 M2 C11 V36
#[test]
fn 새_파일에서도_보호하는_표에_기존_줄을_다시_넣으면_거부된다() {
    let (_dir, path, _judged) = filled_new();
    for table in [
        "agent_sessions",
        "agent_messages",
        "room_events",
        "trigger_receipts",
        "facts",
        "trade_facts",
        "trades",
        "proposals",
        "proposal_evidence",
    ] {
        for rowid in rowids(&path, table) {
            assert_replace_rejected(table, &format!("rowid = {rowid}"), &[]);
        }
    }
}

// ───────────── 키별 우회 ─────────────

// 검증: M1 M2
#[test]
fn 방_로그의_같은_event_id를_다음_번호로_다시_넣어_옛_줄을_지울_수_없다() {
    let filter = format!("room = '{ROOM}' AND seq = 1");
    let next_seq = format!("(SELECT MAX(seq) + 1 FROM room_events WHERE room = '{ROOM}')");
    assert_replace_rejected(
        "room_events",
        &filter,
        &[("seq", &next_seq), ("payload", "'{\"바꿔치기\":true}'")],
    );
}

// 검증: M1 M2
#[test]
fn 방_로그의_같은_번호에_새_event_id를_넣어_옛_줄을_지울_수_없다() {
    for seq in [1, 2] {
        assert_replace_rejected(
            "room_events",
            &format!("room = '{ROOM}' AND seq = {seq}"),
            &[
                ("event_id", "'e-new'"),
                ("payload", "'{\"바꿔치기\":true}'"),
            ],
        );
    }
}

// 검증: M1 M2
#[test]
fn 닫힌_영수증의_같은_작업과_슬롯을_실행_중으로_다시_넣을_수_없다() {
    let started = t(99).0.to_string();
    assert_replace_rejected(
        "trigger_receipts",
        &format!("job = '{CLOSED_JOB}' AND slot = {}", t(1).0),
        &[
            ("status", "'running'"),
            ("delivery", "'not_started'"),
            ("started_at", &started),
            ("finished_at", "NULL"),
        ],
    );
}

// 검증: M1 M2
#[test]
fn 같은_작업의_실행_중_영수증을_다른_슬롯으로_바꿔치기할_수_없다() {
    let slot = t(3).0.to_string();
    let started = t(99).0.to_string();
    assert_replace_rejected(
        "trigger_receipts",
        &format!("job = '{RUNNING_JOB}' AND status = 'running'"),
        &[("slot", &slot), ("started_at", &started)],
    );
    // 실행 중 영수증 자체도 그대로 다시 넣을 수 없다.
    assert_replace_rejected(
        "trigger_receipts",
        &format!("job = '{RUNNING_JOB}' AND status = 'running'"),
        &[],
    );
}

// 검증: M1 M2
#[test]
fn 세션의_같은_id로_다시_넣어_끝난_세션을_되살릴_수_없다() {
    assert_replace_rejected(
        "agent_sessions",
        "ended_at IS NOT NULL",
        &[("ended_at", "NULL"), ("end_reason", "NULL")],
    );
    assert_replace_rejected(
        "agent_sessions",
        "ended_at IS NULL",
        &[("message_count", "0"), ("tool_call_count", "0")],
    );
}

// 검증: M1 M2
#[test]
fn 열린_세션과_같은_담당_제목으로_새_id를_넣어_열린_세션을_지울_수_없다() {
    let started = t(99).0.to_string();
    assert_replace_rejected(
        "agent_sessions",
        &format!("agent = '{AGENT}' AND title = '{OPEN_TITLE}' AND ended_at IS NULL"),
        &[
            ("id", "(SELECT MAX(id) + 1 FROM agent_sessions)"),
            ("started_at", &started),
            ("message_count", "0"),
            ("tool_call_count", "0"),
        ],
    );
}

// 검증: V36 C11
#[test]
fn 자료가_붙은_매매를_같은_id로_다시_넣어_판단_시각을_자료보다_앞으로_옮길_수_없다() {
    let (_dir, _path, judged) = filled_new();
    let earlier = t(1).0.to_string();
    for decided_at in [earlier.as_str(), "NULL"] {
        assert_replace_rejected(
            "trades",
            &format!("id = {}", judged.0),
            &[("decided_at", decided_at)],
        );
    }
}

// 검증: V36 C11
#[test]
fn 매매를_같은_id로_다시_넣어_바꿔치기할_수_없다() {
    let (_dir, path, _judged) = filled_new();
    for id in rowids(&path, "trades") {
        assert_replace_rejected(
            "trades",
            &format!("id = {id}"),
            &[("symbol", "'000660'"), ("state", "state")],
        );
    }
}
