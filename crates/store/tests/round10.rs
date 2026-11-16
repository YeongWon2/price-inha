#![cfg(test)]
//! 10차: 에이전트 세션 · 메시지 · 팀 논의 방 로그 · 트리거 영수증 · 자료(Fact)와 알게 된 시각,
//! 그리고 스키마 버전 6 이전.
//!
//! 원본 근거: `docs/research/agent-runtime-deep.md` 1장 · 5장 · 7.1(Hermes),
//! `docs/research/triggers-approval-memory-deep.md` 1.2(OpenClaw `cron_run_receipts`).
//! 트리거 영수증 규칙은 `crates/trigger/src/store.rs`의 `TriggerStore` 포트와 같다.
//!
//! `v5.db`: `v4.db`를 9차 코드(버전 5)로 열어 올린 뒤 템플릿 1 · 승인 요청 1 · 모의계좌 상태 값 1 ·
//! 접속 키 1세대 · 시각 있는 이벤트 1(`chat` `{"v5":true}`, 14번)을 더 쌓은 파일. 경계는 10이다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use common::{t, temp_store};
use pi_store::{
    AccountKind, AgentMessage, ApprovalId, FactId, FactValue, Judgment, MessageRole, NewFact,
    NewMessage, NewRoomEvent, NewTrade, OpenTriggerReceipt, Rationale, RoomActorKind, RoomAppend,
    SCHEMA_VERSION, SessionId, Store, StoreError, TemplateId, ToolCall, TradeId, TradeState,
    TriggerDelivery, TriggerJob, TriggerReceiptKey, TriggerReceiptStatus,
};
use rusqlite::types::Value as SqlValue;
use serde_json::json;

// ───────────── 도우미 ─────────────

const FIXTURES: [&str; 6] = ["v1.db", "v2.db", "v3.db", "v3_plain.db", "v4.db", "v5.db"];

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

/// `rowid`로 읽을 수 있는 표인가. WITHOUT ROWID 표와 가상 표(FTS5)의 보조 표
/// (`<가상 표 이름>_data` · `_idx` · `_docsize` · `_config` · `_content`)는 거른다.
fn has_rowid(conn: &rusqlite::Connection, name: &str) -> bool {
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .unwrap();
    if sql.to_uppercase().contains("WITHOUT ROWID") {
        return false;
    }
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%'")
        .unwrap();
    let virtuals: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    !virtuals.iter().any(|v| {
        name.strip_prefix(v.as_str())
            .is_some_and(|rest| rest.starts_with('_'))
    })
}

/// 표들의 모든 줄(칸 값 그대로).
fn dump(path: &Path, names: &[String]) -> Vec<(String, Vec<Vec<SqlValue>>)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    names
        .iter()
        .filter(|name| name.as_str() != "sqlite_sequence")
        .filter(|name| has_rowid(&conn, name))
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

fn msg(role: MessageRole, content: &str, at: i64) -> NewMessage {
    NewMessage {
        role,
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

fn tool_call(id: &str, name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: arguments.to_string(),
    }
}

fn tool_result(call_id: &str, name: &str, content: &str, at: i64) -> NewMessage {
    NewMessage {
        tool_call_id: Some(call_id.to_string()),
        tool_name: Some(name.to_string()),
        ..msg(MessageRole::Tool, content, at)
    }
}

/// 메시지가 넣은 것과 칸마다 같은지 본다(번호 · 세션은 따로 본다).
fn assert_same_message(got: &AgentMessage, want: &NewMessage, session: SessionId) {
    assert_eq!(got.session, session);
    assert_eq!(got.role, want.role);
    assert_eq!(got.content, want.content);
    assert_eq!(got.api_content, want.api_content);
    assert_eq!(got.tool_calls, want.tool_calls);
    assert_eq!(got.tool_call_id, want.tool_call_id);
    assert_eq!(got.tool_name, want.tool_name);
    assert_eq!(got.finish_reason, want.finish_reason);
    assert_eq!(got.reasoning, want.reasoning);
    assert_eq!(got.at, want.at);
}

fn room_event(
    room: &str,
    event_id: &str,
    kind: &str,
    actor_kind: RoomActorKind,
    payload: serde_json::Value,
    at: i64,
) -> NewRoomEvent {
    NewRoomEvent {
        room: room.to_string(),
        event_id: event_id.to_string(),
        kind: kind.to_string(),
        actor_kind,
        actor_id: "actor-1".to_string(),
        payload,
        at: t(at),
    }
}

fn appended(result: RoomAppend) -> pi_store::RoomEvent {
    match result {
        RoomAppend::Appended(event) => event,
        other @ RoomAppend::AlreadyExists(_) => panic!("새로 써야 한다: {other:?}"),
    }
}

/// 원본(Hermes `hosted_rooms.py:50-58`)의 쓴 쪽 종류별 이벤트 표.
fn allowed_table() -> Vec<(RoomActorKind, Vec<&'static str>)> {
    vec![
        (RoomActorKind::User, vec!["message.user"]),
        (RoomActorKind::Member, vec!["message.member"]),
        (
            RoomActorKind::Gateway,
            vec![
                "member.unavailable",
                "room.activity",
                "room.assignment",
                "room.continued",
                "room.routed",
                "room.stop_requested",
                "turn.started",
                "turn.settled",
                "turn.failed",
                "turn.cancelled",
                "turn.deferred",
                "turn.reassigned",
            ],
        ),
        (
            RoomActorKind::System,
            vec![
                "authority.claimed",
                "authority.lost",
                "room.context_set",
                "room.created",
                "room.disbanded",
                "room.members_changed",
                "room.renamed",
            ],
        ),
    ]
}

fn key(job: &str, slot: i64) -> TriggerReceiptKey {
    TriggerReceiptKey {
        job: job.to_string(),
        slot: t(slot),
    }
}

fn fact(subject: &str, value: FactValue, known_at: i64) -> NewFact {
    NewFact {
        kind: "financials".to_string(),
        subject: subject.to_string(),
        label: "매출액".to_string(),
        period: Some("2026-09-30".to_string()),
        value,
        unit: Some("KRW".to_string()),
        reference: Some("20261114000123".to_string()),
        source_name: "dart".to_string(),
        source_url: "http://127.0.0.1:9/api/fnlttSinglAcnt.json?crtfc_key=***".to_string(),
        published_on: Some("2026-11-14".to_string()),
        known_at: t(known_at),
    }
}

fn number(text: &str) -> FactValue {
    FactValue::Number(text.to_string())
}

/// 판단 시각 `decided_at`으로 판단된 매매 하나.
fn judged_trade(store: &Store, decided_at: i64) -> TradeId {
    let trade = store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap();
    store
        .judge_trade(
            trade.id,
            Judgment {
                decided_at: t(decided_at),
                rationale: Rationale::default(),
                sources: Vec::new(),
                scores: Vec::new(),
                consensus_score: 70,
            },
        )
        .unwrap();
    trade.id
}

fn assert_new_tables_empty_and_usable(store: &Store, label: &str) {
    assert!(
        store.sessions_of("시세 담당").unwrap().is_empty(),
        "{label}"
    );
    assert!(
        store.room_events("팀", 0, 100).unwrap().is_empty(),
        "{label}"
    );
    assert!(store.trigger_jobs().unwrap().is_empty(), "{label}");
    assert!(store.trigger_receipts().unwrap().is_empty(), "{label}");
    assert!(
        store
            .facts_known_by("005930", t(1_000_000))
            .unwrap()
            .is_empty(),
        "{label}"
    );
    let session = store.open_session("시세 담당", "팀", t(1)).unwrap();
    assert_eq!(session.id, SessionId(1), "{label}");
    let written = store
        .append_messages(session.id, &[msg(MessageRole::User, "안녕", 2)])
        .unwrap();
    assert_eq!(written.len(), 1, "{label}");
    let event = appended(
        store
            .append_room_event(room_event(
                "팀",
                "e1",
                "message.user",
                RoomActorKind::User,
                json!({ "text": "hi" }),
                3,
            ))
            .unwrap(),
    );
    assert_eq!(event.seq, 1, "{label}");
    assert_eq!(
        store.open_trigger_receipt(&key("job", 4), t(4)).unwrap(),
        OpenTriggerReceipt::Opened,
        "{label}"
    );
    let saved = store.save_facts(&[fact("005930", number("1"), 5)]).unwrap();
    assert_eq!(saved[0].id, FactId(1), "{label}");
}

// ───────────── 세션 · 메시지 ─────────────

// 검증: D6
#[test]
fn 같은_담당과_제목이면_같은_세션을_이어_쓰고_담당이_다르면_다른_세션이다() {
    let (_dir, _path, store) = temp_store();
    let first = store.open_session("시세 담당", "팀 논의", t(10)).unwrap();
    assert_eq!(first.id, SessionId(1));
    assert_eq!(first.agent, "시세 담당");
    assert_eq!(first.title, "팀 논의");
    assert_eq!(first.started_at, t(10));
    assert_eq!(first.ended_at, None);
    assert_eq!(first.end_reason, None);
    assert_eq!(first.message_count, 0);
    assert_eq!(first.tool_call_count, 0);

    // 같은 담당 · 같은 제목: 같은 세션(연 시각은 처음 것).
    let again = store.open_session("시세 담당", "팀 논의", t(99)).unwrap();
    assert_eq!(again, first);
    // 담당이 다르면 제목이 같아도 다른 세션.
    let other_agent = store.open_session("재무 담당", "팀 논의", t(11)).unwrap();
    assert_eq!(other_agent.id, SessionId(2));
    // 같은 담당의 다른 제목도 다른 세션.
    let other_title = store
        .open_session("시세 담당", "사용자 대화", t(12))
        .unwrap();
    assert_eq!(other_title.id, SessionId(3));

    assert_eq!(store.get_session(SessionId(2)).unwrap(), other_agent);
    assert_eq!(
        store.sessions_of("시세 담당").unwrap(),
        vec![first.clone(), other_title]
    );
    assert_eq!(store.sessions_of("재무 담당").unwrap(), vec![other_agent]);
    assert!(store.sessions_of("없는 담당").unwrap().is_empty());
}

// 검증: D6
#[test]
fn 세션을_끝내면_같은_제목으로_새_세션이_열리고_다시_끝낼_수_없다() {
    let (_dir, _path, store) = temp_store();
    let first = store.open_session("시세 담당", "팀 논의", t(10)).unwrap();
    let ended = store.end_session(first.id, "session_reset", t(20)).unwrap();
    assert_eq!(ended.ended_at, Some(t(20)));
    assert_eq!(ended.end_reason.as_deref(), Some("session_reset"));
    assert_eq!(store.get_session(first.id).unwrap(), ended);

    let second = store.open_session("시세 담당", "팀 논의", t(30)).unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(second.started_at, t(30));
    assert_eq!(second.ended_at, None);

    assert!(matches!(
        store.end_session(first.id, "again", t(40)),
        Err(StoreError::SessionEnded(id)) if id == first.id
    ));
    assert_eq!(store.get_session(first.id).unwrap(), ended);
    // 끝낸 세션에는 쓸 수 없다. 아무것도 쓰지 않는다.
    assert!(matches!(
        store.append_messages(first.id, &[msg(MessageRole::User, "늦은 글", 41)]),
        Err(StoreError::SessionEnded(id)) if id == first.id
    ));
    assert!(store.session_messages(first.id).unwrap().is_empty());
    assert_eq!(store.get_session(first.id).unwrap().message_count, 0);
}

// 검증: D6
#[test]
fn 없는_세션은_찾을_수_없다는_오류다() {
    let (_dir, _path, store) = temp_store();
    let missing = SessionId(42);
    assert!(matches!(
        store.get_session(missing),
        Err(StoreError::SessionNotFound(id)) if id == missing
    ));
    assert!(matches!(
        store.end_session(missing, "x", t(1)),
        Err(StoreError::SessionNotFound(id)) if id == missing
    ));
    assert!(matches!(
        store.append_messages(missing, &[msg(MessageRole::User, "x", 1)]),
        Err(StoreError::SessionNotFound(id)) if id == missing
    ));
    assert!(matches!(
        store.session_messages(missing),
        Err(StoreError::SessionNotFound(id)) if id == missing
    ));
}

// 검증: D6, M2
#[test]
fn 도구를_부르기_전에_도구_호출_줄을_먼저_쓰고_결과를_뒤에_쓴다() {
    let (_dir, _path, store) = temp_store();
    let session = store.open_session("주문 담당", "매매 7", t(0)).unwrap();

    let user = msg(MessageRole::User, "삼성전자 살까?", 1);
    let call = NewMessage {
        content: None,
        tool_calls: vec![
            tool_call("call_1", "quote", r#"{"symbol":"005930"}"#),
            // 모델이 준 인자는 JSON이 아니어도 글자 그대로 둔다.
            tool_call("call_2", "place_paper_order", "{\"qty\": 3,"),
        ],
        finish_reason: Some("tool_calls".to_string()),
        reasoning: Some("시세를 먼저 본다".to_string()),
        api_content: Some("api: 원문 그대로 \u{0}\t끝".to_string()),
        ..msg(MessageRole::Assistant, "", 2)
    };
    let first = store
        .append_messages(session.id, &[user.clone(), call.clone()])
        .unwrap();
    assert_eq!(first.len(), 2);
    assert_same_message(&first[0], &user, session.id);
    assert_same_message(&first[1], &call, session.id);
    assert!(first[0].id < first[1].id);

    // 도구 결과가 아직 없어도(짝 없는 호출) 저장되어 있다: 도구를 돌리기 전에 읽힌다.
    let stored = store.session_messages(session.id).unwrap();
    assert_eq!(stored, first);
    let counted = store.get_session(session.id).unwrap();
    assert_eq!(counted.message_count, 2);
    assert_eq!(counted.tool_call_count, 2);

    let results = [
        tool_result("call_1", "quote", r#"{"price":"71500"}"#, 3),
        tool_result("call_2", "place_paper_order", "취소됨", 4),
    ];
    let second = store.append_messages(session.id, &results).unwrap();
    assert_eq!(second.len(), 2);
    assert_same_message(&second[0], &results[0], session.id);
    assert_same_message(&second[1], &results[1], session.id);
    assert_eq!(second[0].id.0, first[1].id.0 + 1);
    assert_eq!(second[1].id.0, first[1].id.0 + 2);

    let all = store.session_messages(session.id).unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(all[..2], first[..]);
    assert_eq!(all[2..], second[..]);
    let counted = store.get_session(session.id).unwrap();
    assert_eq!(counted.message_count, 4);
    assert_eq!(counted.tool_call_count, 2);
}

// 검증: D6, M2
#[test]
fn 묶음에_규칙을_어긴_메시지가_하나라도_있으면_아무것도_쓰지_않는다() {
    let (_dir, _path, store) = temp_store();
    let session = store.open_session("주문 담당", "매매 7", t(0)).unwrap();
    let written = store
        .append_messages(session.id, &[msg(MessageRole::User, "처음", 1)])
        .unwrap();
    let before = store.get_session(session.id).unwrap();

    // 도구 결과인데 호출 id가 없다(세 번째).
    let bad_tool = NewMessage {
        tool_call_id: None,
        ..tool_result("call_1", "quote", "x", 4)
    };
    let batch = [
        NewMessage {
            tool_calls: vec![tool_call("call_1", "quote", "{}")],
            ..msg(MessageRole::Assistant, "", 2)
        },
        msg(MessageRole::User, "가운데", 3),
        bad_tool,
    ];
    assert!(matches!(
        store.append_messages(session.id, &batch),
        Err(StoreError::InvalidMessage { index: 2 })
    ));
    // 도구 호출은 assistant만 가진다(첫 번째).
    let user_with_calls = NewMessage {
        tool_calls: vec![tool_call("call_9", "quote", "{}")],
        ..msg(MessageRole::User, "x", 5)
    };
    assert!(matches!(
        store.append_messages(
            session.id,
            &[user_with_calls, msg(MessageRole::User, "y", 6)]
        ),
        Err(StoreError::InvalidMessage { index: 0 })
    ));
    let tool_with_calls = NewMessage {
        tool_calls: vec![tool_call("call_8", "quote", "{}")],
        ..tool_result("call_1", "quote", "x", 7)
    };
    assert!(matches!(
        store.append_messages(session.id, &[tool_with_calls]),
        Err(StoreError::InvalidMessage { index: 0 })
    ));

    assert_eq!(store.session_messages(session.id).unwrap(), written);
    assert_eq!(store.get_session(session.id).unwrap(), before);
    // 번호를 쓰지 않았다: 다음 메시지는 바로 뒤 번호다.
    let next = store
        .append_messages(session.id, &[msg(MessageRole::User, "다음", 8)])
        .unwrap();
    assert_eq!(next[0].id.0, written[0].id.0 + 1);
}

// 검증: D6
#[test]
fn 빈_묶음은_아무것도_쓰지_않는다() {
    let (_dir, _path, store) = temp_store();
    let session = store.open_session("시세 담당", "팀", t(0)).unwrap();
    assert!(store.append_messages(session.id, &[]).unwrap().is_empty());
    assert!(store.session_messages(session.id).unwrap().is_empty());
    assert_eq!(store.get_session(session.id).unwrap(), session);
}

// 검증: D6
#[test]
fn 메시지_번호는_세션을_통틀어_빠짐없이_커지고_세션마다_나뉘어_읽힌다() {
    let (_dir, _path, store) = temp_store();
    let a = store.open_session("시세 담당", "팀", t(0)).unwrap();
    let b = store.open_session("재무 담당", "팀", t(0)).unwrap();
    let mut ids = Vec::new();
    for n in 0..3 {
        let session = if n % 2 == 0 { a.id } else { b.id };
        let written = store
            .append_messages(session, &[msg(MessageRole::User, &format!("{n}"), n)])
            .unwrap();
        ids.push(written[0].id.0);
    }
    assert_eq!(ids, vec![1, 2, 3]);
    let in_a: Vec<i64> = store
        .session_messages(a.id)
        .unwrap()
        .iter()
        .map(|m| m.id.0)
        .collect();
    let in_b: Vec<i64> = store
        .session_messages(b.id)
        .unwrap()
        .iter()
        .map(|m| m.id.0)
        .collect();
    assert_eq!(in_a, vec![1, 3]);
    assert_eq!(in_b, vec![2]);
}

// 검증: D6
#[test]
fn 다시_열어도_세션과_메시지가_그대로이고_번호가_이어진다() {
    let (_dir, path, store) = temp_store();
    let session = store.open_session("시세 담당", "팀", t(0)).unwrap();
    let written = store
        .append_messages(
            session.id,
            &[
                msg(MessageRole::System, "너는 시세 담당이다", 1),
                NewMessage {
                    tool_calls: vec![tool_call("c1", "quote", "{}")],
                    ..msg(MessageRole::Assistant, "", 2)
                },
            ],
        )
        .unwrap();
    let saved = store.get_session(session.id).unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_session(session.id).unwrap(), saved);
    assert_eq!(store.session_messages(session.id).unwrap(), written);
    // 끝내기 전이므로 같은 제목으로 이어 쓴다.
    assert_eq!(store.open_session("시세 담당", "팀", t(50)).unwrap(), saved);
    let next = store
        .append_messages(session.id, &[tool_result("c1", "quote", "1", 3)])
        .unwrap();
    assert_eq!(next[0].id.0, written[1].id.0 + 1);
    let next_session = store.open_session("재무 담당", "팀", t(51)).unwrap();
    assert_eq!(next_session.id, SessionId(2));
}

// ───────────── 팀 논의 방 로그 ─────────────

// 검증: D6
#[test]
fn 방마다_번호가_1부터_빠짐없이_커지고_쪽으로_읽힌다() {
    let (_dir, _path, store) = temp_store();
    for n in 1..=5 {
        let event = appended(
            store
                .append_room_event(room_event(
                    "팀",
                    &format!("u{n}"),
                    "message.user",
                    RoomActorKind::User,
                    json!({ "text": format!("말 {n}"), "thread_id": "t1" }),
                    n,
                ))
                .unwrap(),
        );
        assert_eq!(event.seq, u64::try_from(n).unwrap());
        assert_eq!(event.room, "팀");
        assert_eq!(event.event_id, format!("u{n}"));
        assert_eq!(event.kind, "message.user");
        assert_eq!(event.actor_kind, RoomActorKind::User);
        assert_eq!(event.actor_id, "actor-1");
        assert_eq!(event.at, t(n));
    }
    // 다른 방은 따로 센다.
    let other = appended(
        store
            .append_room_event(room_event(
                "다른 방",
                "u1",
                "message.user",
                RoomActorKind::User,
                json!(null),
                9,
            ))
            .unwrap(),
    );
    assert_eq!(other.seq, 1);

    let all = store.room_events("팀", 0, 100).unwrap();
    assert_eq!(
        all.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(all[2].payload, json!({ "text": "말 3", "thread_id": "t1" }));
    let page = store.room_events("팀", 2, 2).unwrap();
    assert_eq!(page, all[2..4]);
    assert!(store.room_events("팀", 5, 10).unwrap().is_empty());
    assert!(store.room_events("팀", 0, 0).unwrap().is_empty());
    assert!(store.room_events("없는 방", 0, 10).unwrap().is_empty());
    assert_eq!(store.room_events("다른 방", 0, 10).unwrap(), vec![other]);
}

// 검증: D6
#[test]
fn 같은_멱등_키로_같은_내용이면_있던_줄을_주고_다른_내용이면_거부한다() {
    let (_dir, _path, store) = temp_store();
    let first = appended(
        store
            .append_room_event(room_event(
                "팀",
                "dmessage:abc",
                "message.member",
                RoomActorKind::Member,
                json!({ "text": "산다", "turn_id": "d1.r0.p0" }),
                10,
            ))
            .unwrap(),
    );
    // 같은 내용(시각만 다름): 있던 줄 그대로, 새 번호를 쓰지 않는다.
    let retry = store
        .append_room_event(room_event(
            "팀",
            "dmessage:abc",
            "message.member",
            RoomActorKind::Member,
            json!({ "turn_id": "d1.r0.p0", "text": "산다" }),
            20,
        ))
        .unwrap();
    assert_eq!(retry, RoomAppend::AlreadyExists(first.clone()));

    // 내용이 다르면 거부.
    let conflicts = [
        room_event(
            "팀",
            "dmessage:abc",
            "message.member",
            RoomActorKind::Member,
            json!({ "text": "판다", "turn_id": "d1.r0.p0" }),
            30,
        ),
        room_event(
            "팀",
            "dmessage:abc",
            "turn.settled",
            RoomActorKind::Gateway,
            json!({ "text": "산다", "turn_id": "d1.r0.p0" }),
            31,
        ),
        NewRoomEvent {
            actor_id: "다른 멤버".to_string(),
            ..room_event(
                "팀",
                "dmessage:abc",
                "message.member",
                RoomActorKind::Member,
                json!({ "text": "산다", "turn_id": "d1.r0.p0" }),
                32,
            )
        },
    ];
    for conflict in conflicts {
        assert!(matches!(
            store.append_room_event(conflict),
            Err(StoreError::RoomEventConflict { room, event_id })
                if room == "팀" && event_id == "dmessage:abc"
        ));
    }
    assert_eq!(store.room_events("팀", 0, 10).unwrap(), vec![first]);

    // 다른 방의 같은 멱등 키는 겹치지 않는다.
    let elsewhere = appended(
        store
            .append_room_event(room_event(
                "다른 방",
                "dmessage:abc",
                "message.member",
                RoomActorKind::Member,
                json!({ "text": "판다" }),
                40,
            ))
            .unwrap(),
    );
    assert_eq!(elsewhere.seq, 1);
    // 다음 새 이벤트는 2번이다(거부와 되풀이가 번호를 쓰지 않았다).
    let next = appended(
        store
            .append_room_event(room_event(
                "팀",
                "u2",
                "message.user",
                RoomActorKind::User,
                json!({}),
                50,
            ))
            .unwrap(),
    );
    assert_eq!(next.seq, 2);
}

// 검증: D6
#[test]
fn 쓴_쪽의_종류마다_정해진_이벤트만_쓸_수_있다() {
    let (_dir, _path, store) = temp_store();
    let table = allowed_table();
    let mut seq = 0;
    for (actor, kinds) in &table {
        for kind in kinds {
            seq += 1;
            let event = appended(
                store
                    .append_room_event(room_event(
                        "팀",
                        &format!("ok-{seq}"),
                        kind,
                        *actor,
                        json!({}),
                        seq,
                    ))
                    .unwrap(),
            );
            assert_eq!(event.kind, *kind);
        }
    }
    // 표에 없는 짝: 다른 쪽의 종류 전부와 표에 없는 종류.
    let actors = [
        RoomActorKind::User,
        RoomActorKind::Member,
        RoomActorKind::Gateway,
        RoomActorKind::System,
    ];
    for actor in actors {
        let own: &Vec<&str> = &table.iter().find(|(a, _)| *a == actor).unwrap().1;
        let mut denied: Vec<&str> = table
            .iter()
            .flat_map(|(_, kinds)| kinds.iter().copied())
            .filter(|kind| !own.contains(kind))
            .collect();
        denied.extend(["chat", "message", "", "MESSAGE.USER"]);
        for kind in denied {
            assert!(
                matches!(
                    store.append_room_event(room_event(
                        "팀",
                        &format!("no-{actor:?}-{kind}"),
                        kind,
                        actor,
                        json!({}),
                        1,
                    )),
                    Err(StoreError::RoomEventNotAllowed { actor_kind, kind: got })
                        if actor_kind == actor && got == kind
                ),
                "{actor:?} {kind}"
            );
        }
    }
    // 거부는 번호를 쓰지 않았다.
    let all = store.room_events("팀", 0, 1000).unwrap();
    assert_eq!(all.len(), seq as usize);
    assert_eq!(all.last().unwrap().seq, u64::try_from(seq).unwrap());
}

// 검증: D6
#[test]
fn 여러_스레드가_함께_써도_방_번호가_겹치거나_비지_않고_다시_열어도_이어진다() {
    let (_dir, path, store) = temp_store();
    let store = Arc::new(store);
    let handles: Vec<_> = (0..4)
        .map(|worker| {
            let store = Arc::clone(&store);
            thread::spawn(move || {
                for n in 0..25 {
                    appended(
                        store
                            .append_room_event(room_event(
                                "팀",
                                &format!("w{worker}-{n}"),
                                "message.user",
                                RoomActorKind::User,
                                json!({ "w": worker, "n": n }),
                                n,
                            ))
                            .unwrap(),
                    );
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let all = store.room_events("팀", 0, 1000).unwrap();
    assert_eq!(
        all.iter().map(|e| e.seq).collect::<Vec<_>>(),
        (1..=100).collect::<Vec<u64>>()
    );
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.room_events("팀", 0, 1000).unwrap(), all);
    let next = appended(
        store
            .append_room_event(room_event(
                "팀",
                "after-reopen",
                "room.renamed",
                RoomActorKind::System,
                json!({ "name": "새 이름" }),
                1,
            ))
            .unwrap(),
    );
    assert_eq!(next.seq, 101);
    // 다시 연 뒤에도 멱등 키를 기억한다.
    assert!(matches!(
        store.append_room_event(room_event(
            "팀",
            "w0-0",
            "message.user",
            RoomActorKind::User,
            json!({ "w": 0, "n": 0 }),
            7,
        )),
        Ok(RoomAppend::AlreadyExists(event)) if event.seq == all.iter().find(|e| e.event_id == "w0-0").unwrap().seq
    ));
}

// ───────────── 트리거 영수증 ─────────────

// 검증: D6
#[test]
fn 같은_회차는_한_번만_열리고_작업마다_실행_중은_하나다() {
    let (_dir, _path, store) = temp_store();
    let first = key("daily-review", 1_000);
    assert_eq!(
        store.open_trigger_receipt(&first, t(1_001)).unwrap(),
        OpenTriggerReceipt::Opened
    );
    // 같은 키: 실행 중이어도 Duplicate가 먼저다.
    assert_eq!(
        store.open_trigger_receipt(&first, t(1_002)).unwrap(),
        OpenTriggerReceipt::Duplicate
    );
    // 같은 작업의 다른 회차: 실행 중인 것의 키.
    assert_eq!(
        store
            .open_trigger_receipt(&key("daily-review", 2_000), t(2_001))
            .unwrap(),
        OpenTriggerReceipt::AlreadyRunning(first.clone())
    );
    // 다른 작업은 따로다.
    let other = key("price-watch", 1_000);
    assert_eq!(
        store.open_trigger_receipt(&other, t(1_003)).unwrap(),
        OpenTriggerReceipt::Opened
    );

    let receipts = store.trigger_receipts().unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].key, first);
    assert_eq!(receipts[0].status, TriggerReceiptStatus::Running);
    assert_eq!(receipts[0].delivery, TriggerDelivery::NotStarted);
    assert_eq!(receipts[0].started_at, t(1_001));
    assert_eq!(receipts[0].finished_at, None);
    assert_eq!(receipts[1].key, other);
    assert_eq!(receipts[1].started_at, t(1_003));
}

// 검증: D6
#[test]
fn 닫으면_다음_회차가_열리고_닫힌_회차는_다시_열리지_않는다() {
    let (_dir, _path, store) = temp_store();
    let first = key("daily-review", 1_000);
    store.open_trigger_receipt(&first, t(1_001)).unwrap();
    store.mark_trigger_delivery_started(&first).unwrap();
    store
        .close_trigger_receipt(&first, TriggerReceiptStatus::Ok, t(1_500))
        .unwrap();
    let closed = &store.trigger_receipts().unwrap()[0];
    assert_eq!(closed.status, TriggerReceiptStatus::Ok);
    assert_eq!(closed.delivery, TriggerDelivery::Started);
    assert_eq!(closed.finished_at, Some(t(1_500)));
    assert_eq!(closed.started_at, t(1_001));

    assert_eq!(
        store.open_trigger_receipt(&first, t(1_600)).unwrap(),
        OpenTriggerReceipt::Duplicate
    );
    let second = key("daily-review", 2_000);
    assert_eq!(
        store.open_trigger_receipt(&second, t(2_001)).unwrap(),
        OpenTriggerReceipt::Opened
    );
    for (status, finished) in [
        (TriggerReceiptStatus::Error, 2_100),
        (TriggerReceiptStatus::Interrupted, 3_100),
    ] {
        let slot = finished - 100;
        let k = key("daily-review", slot);
        if slot != 2_000 {
            assert_eq!(
                store.open_trigger_receipt(&k, t(slot + 1)).unwrap(),
                OpenTriggerReceipt::Opened
            );
        }
        store
            .close_trigger_receipt(&k, status, t(finished))
            .unwrap();
        let got = store
            .trigger_receipts()
            .unwrap()
            .into_iter()
            .find(|r| r.key == k)
            .unwrap();
        assert_eq!(got.status, status);
        assert_eq!(got.finished_at, Some(t(finished)));
        assert_eq!(got.delivery, TriggerDelivery::NotStarted);
    }
    assert_eq!(store.trigger_receipts().unwrap().len(), 3);
}

// 검증: D6
#[test]
fn 실행_중으로_닫거나_없는_영수증을_다루면_거부하고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let first = key("daily-review", 1_000);
    store.open_trigger_receipt(&first, t(1_001)).unwrap();
    let before = store.trigger_receipts().unwrap();
    assert!(matches!(
        store.close_trigger_receipt(&first, TriggerReceiptStatus::Running, t(1_500)),
        Err(StoreError::TriggerReceiptRunningClose(k)) if k == first
    ));
    assert_eq!(store.trigger_receipts().unwrap(), before);

    let missing = key("daily-review", 9_000);
    assert!(matches!(
        store.close_trigger_receipt(&missing, TriggerReceiptStatus::Ok, t(9_001)),
        Err(StoreError::TriggerReceiptNotFound(k)) if k == missing
    ));
    assert!(matches!(
        store.mark_trigger_delivery_started(&missing),
        Err(StoreError::TriggerReceiptNotFound(k)) if k == missing
    ));
    assert_eq!(store.trigger_receipts().unwrap(), before);
}

// 검증: D6
#[test]
fn 상태_저장과_영수증_열기는_한_단위다() {
    let (_dir, _path, store) = temp_store();
    let job = "daily-review";
    let state_1 = json!({ "last_slot": 1_000, "pending_slot": 1_000, "consecutive_errors": 0 });
    let first = key(job, 1_000);
    // Opened: 상태와 영수증 둘 다.
    assert_eq!(
        store
            .save_job_and_open_receipt(job, &state_1, &first, t(1_001))
            .unwrap(),
        OpenTriggerReceipt::Opened
    );
    assert_eq!(
        store.trigger_jobs().unwrap(),
        vec![TriggerJob {
            job: job.to_string(),
            state: state_1.clone(),
        }]
    );
    assert_eq!(store.trigger_receipts().unwrap().len(), 1);

    // AlreadyRunning: 아무것도 저장하지 않는다.
    let state_2 = json!({ "last_slot": 2_000, "pending_slot": 2_000 });
    assert_eq!(
        store
            .save_job_and_open_receipt(job, &state_2, &key(job, 2_000), t(2_001))
            .unwrap(),
        OpenTriggerReceipt::AlreadyRunning(first.clone())
    );
    assert_eq!(store.trigger_jobs().unwrap()[0].state, state_1);
    assert_eq!(store.trigger_receipts().unwrap().len(), 1);

    // Duplicate: 상태는 저장하고 영수증은 그대로.
    store
        .close_trigger_receipt(&first, TriggerReceiptStatus::Ok, t(1_500))
        .unwrap();
    let receipts_before = store.trigger_receipts().unwrap();
    let state_3 = json!({ "last_slot": 1_000, "pending_slot": null });
    assert_eq!(
        store
            .save_job_and_open_receipt(job, &state_3, &first, t(3_000))
            .unwrap(),
        OpenTriggerReceipt::Duplicate
    );
    assert_eq!(store.trigger_jobs().unwrap()[0].state, state_3);
    assert_eq!(store.trigger_receipts().unwrap(), receipts_before);

    // 닫힌 뒤 다음 회차는 열린다.
    assert_eq!(
        store
            .save_job_and_open_receipt(job, &state_2, &key(job, 2_000), t(2_001))
            .unwrap(),
        OpenTriggerReceipt::Opened
    );
    assert_eq!(store.trigger_jobs().unwrap()[0].state, state_2);
}

// 검증: D6
#[test]
fn 작업_상태는_id마다_하나이고_넣은_그대로_나온다() {
    let (_dir, _path, store) = temp_store();
    let b = json!({ "condition": { "armed": true }, "catch_up": [1, 2], "big": 9_007_199_254_740_993_i64 });
    store.save_trigger_job("b-job", &b).unwrap();
    store.save_trigger_job("a-job", &json!({ "v": 1 })).unwrap();
    store.save_trigger_job("a-job", &json!({ "v": 2 })).unwrap();
    assert_eq!(
        store.trigger_jobs().unwrap(),
        vec![
            TriggerJob {
                job: "a-job".to_string(),
                state: json!({ "v": 2 }),
            },
            TriggerJob {
                job: "b-job".to_string(),
                state: b,
            },
        ]
    );
}

// 검증: D6
#[test]
fn 다시_켜도_실행_중_영수증과_작업_상태가_그대로_남는다() {
    let (_dir, path, store) = temp_store();
    let running = key("daily-review", 1_000);
    store
        .save_job_and_open_receipt(
            "daily-review",
            &json!({ "pending_slot": 1_000 }),
            &running,
            t(1_001),
        )
        .unwrap();
    store.mark_trigger_delivery_started(&running).unwrap();
    let closed = key("price-watch", 500);
    store.open_trigger_receipt(&closed, t(501)).unwrap();
    store
        .close_trigger_receipt(&closed, TriggerReceiptStatus::Error, t(600))
        .unwrap();
    let receipts = store.trigger_receipts().unwrap();
    let jobs = store.trigger_jobs().unwrap();
    // 갑자기 꺼짐: 손잡이를 닫지 않고 버린다.
    std::mem::forget(store);

    let store = Store::open(&path).unwrap();
    // 저장소는 실행 중 영수증을 스스로 닫지 않는다(다시 켠 뒤 닫는 것은 trigger의 일).
    assert_eq!(store.trigger_receipts().unwrap(), receipts);
    assert_eq!(store.trigger_jobs().unwrap(), jobs);
    assert_eq!(
        store
            .open_trigger_receipt(&key("daily-review", 2_000), t(2_001))
            .unwrap(),
        OpenTriggerReceipt::AlreadyRunning(running.clone())
    );
    assert_eq!(
        store.open_trigger_receipt(&closed, t(700)).unwrap(),
        OpenTriggerReceipt::Duplicate
    );
    store
        .close_trigger_receipt(&running, TriggerReceiptStatus::Interrupted, t(3_000))
        .unwrap();
    assert_eq!(
        store
            .open_trigger_receipt(&key("daily-review", 2_000), t(3_001))
            .unwrap(),
        OpenTriggerReceipt::Opened
    );
}

// 검증: D6
#[test]
fn 영수증_표는_sql로도_규칙을_어길_수_없다() {
    let (_dir, path, store) = temp_store();
    let done = key("daily-review", 1_000);
    store.open_trigger_receipt(&done, t(1_001)).unwrap();
    store
        .close_trigger_receipt(&done, TriggerReceiptStatus::Ok, t(1_500))
        .unwrap();
    let running = key("daily-review", 2_000);
    store.open_trigger_receipt(&running, t(2_001)).unwrap();
    let before = store.trigger_receipts().unwrap();
    drop(store);

    let done_slot = done.slot.0;
    let running_slot = running.slot.0;
    let conn = rusqlite::Connection::open(&path).unwrap();
    let attempts = [
        // 상태 값은 넷 가운데 하나.
        format!("UPDATE trigger_receipts SET status = 'bogus' WHERE slot = {done_slot}"),
        // 닫힌 줄은 닫은 시각이 있어야 한다.
        format!("UPDATE trigger_receipts SET finished_at = NULL WHERE slot = {done_slot}"),
        // 실행 중인 줄은 닫은 시각이 없어야 한다.
        format!("UPDATE trigger_receipts SET finished_at = 5 WHERE slot = {running_slot}"),
        // 한 작업에 실행 중은 하나뿐.
        format!(
            "UPDATE trigger_receipts SET status = 'running', finished_at = NULL WHERE slot = {done_slot}"
        ),
        // (작업, 예정 시각)은 겹치지 않는다.
        format!("UPDATE trigger_receipts SET slot = {running_slot} WHERE slot = {done_slot}"),
    ];
    for sql in &attempts {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.trigger_receipts().unwrap(), before);
}

// ───────────── 자료(Fact)와 알게 된 시각 (C11 · V36) ─────────────

// 검증: C11
#[test]
fn 자료는_알게_된_시각과_함께_칸마다_넣은_그대로_저장된다() {
    let (_dir, _path, store) = temp_store();
    let facts = [
        fact("005930", number("-12345678901234567890.1234567890"), 100),
        fact("005930", number("0.10"), 101),
        NewFact {
            kind: "disclosure".to_string(),
            label: "삼성전자".to_string(),
            period: None,
            value: FactValue::Text("분기보고서 (2026.09)".to_string()),
            unit: None,
            reference: None,
            published_on: None,
            ..fact("00126380", number("0"), 102)
        },
    ];
    let saved = store.save_facts(&facts).unwrap();
    assert_eq!(saved.len(), 3);
    for (n, (got, want)) in saved.iter().zip(facts.iter()).enumerate() {
        assert_eq!(got.id, FactId(i64::try_from(n).unwrap() + 1));
        assert_eq!(got.kind, want.kind);
        assert_eq!(got.subject, want.subject);
        assert_eq!(got.label, want.label);
        assert_eq!(got.period, want.period);
        assert_eq!(got.value, want.value);
        assert_eq!(got.unit, want.unit);
        assert_eq!(got.reference, want.reference);
        assert_eq!(got.source_name, want.source_name);
        assert_eq!(got.source_url, want.source_url);
        assert_eq!(got.published_on, want.published_on);
        assert_eq!(got.known_at, want.known_at);
        assert_eq!(store.get_fact(got.id).unwrap(), *got);
    }
    // 수 값은 글자 하나 바뀌지 않는다(부동소수점을 거치지 않는다).
    assert_eq!(
        store.get_fact(FactId(1)).unwrap().value,
        number("-12345678901234567890.1234567890")
    );
    assert_eq!(store.get_fact(FactId(2)).unwrap().value, number("0.10"));

    assert!(matches!(
        store.get_fact(FactId(99)),
        Err(StoreError::FactNotFound(id)) if id == FactId(99)
    ));
    // 빈 묶음은 아무것도 쓰지 않는다. 번호는 이어진다.
    assert!(store.save_facts(&[]).unwrap().is_empty());
    let next = store
        .save_facts(&[fact("005930", number("1"), 103)])
        .unwrap();
    assert_eq!(next[0].id, FactId(4));
}

// 검증: C11, V36
#[test]
fn 판단_시각에_이미_알_수_있던_자료만_나온다() {
    let (_dir, _path, store) = temp_store();
    let saved = store
        .save_facts(&[
            fact("005930", number("1"), 100),
            fact("005930", number("2"), 200),
            fact("005930", number("3"), 201),
            fact("000660", number("4"), 50),
            fact("005930", number("5"), 150),
        ])
        .unwrap();
    let ids = |facts: Vec<pi_store::StoredFact>| facts.iter().map(|f| f.id.0).collect::<Vec<_>>();
    // 같은 시각은 든다. 늦은 것은 빠진다. 다른 대상은 빠진다. 번호 순서다.
    assert_eq!(
        ids(store.facts_known_by("005930", t(200)).unwrap()),
        vec![1, 2, 5]
    );
    assert_eq!(
        ids(store.facts_known_by("005930", t(199)).unwrap()),
        vec![1, 5]
    );
    assert_eq!(
        ids(store.facts_known_by("005930", t(99)).unwrap()),
        Vec::<i64>::new()
    );
    assert_eq!(
        ids(store.facts_known_by("005930", t(10_000)).unwrap()),
        vec![1, 2, 3, 5]
    );
    assert_eq!(ids(store.facts_known_by("000660", t(50)).unwrap()), vec![4]);
    let got = store.facts_known_by("005930", t(200)).unwrap();
    assert!(got.iter().all(|f| f.known_at <= t(200)));
    assert_eq!(got[0], saved[0]);
}

// 검증: V36, M2
#[test]
fn 매매에는_판단_시각까지_알게_된_자료만_근거로_붙는다() {
    let (_dir, _path, store) = temp_store();
    let trade = judged_trade(&store, 1_000);
    let saved = store
        .save_facts(&[
            fact("005930", number("1"), 900),
            fact("005930", number("2"), 1_000),
            fact("005930", number("3"), 1_001),
        ])
        .unwrap();
    let (early, same, late) = (saved[0].id, saved[1].id, saved[2].id);

    let attached = store.attach_trade_facts(trade, &[same, early]).unwrap();
    assert_eq!(attached, vec![saved[1].clone(), saved[0].clone()]);
    assert_eq!(store.trade_facts(trade).unwrap(), attached);

    // 늦은 자료가 하나라도 있으면 아무것도 붙이지 않는다.
    let fresh = store
        .save_facts(&[fact("005930", number("4"), 10)])
        .unwrap()[0]
        .id;
    assert!(matches!(
        store.attach_trade_facts(trade, &[fresh, late]),
        Err(StoreError::FactAfterDecision { fact, known_at, decided_at })
            if fact == late && known_at == t(1_001) && decided_at == t(1_000)
    ));
    assert_eq!(store.trade_facts(trade).unwrap(), attached);

    // 다시 주면 한 번만 남는다.
    let again = store.attach_trade_facts(trade, &[early, fresh]).unwrap();
    assert_eq!(
        again.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![same, early, fresh]
    );
    assert_eq!(store.trade_facts(trade).unwrap(), again);
    // 붙은 자료는 하나도 판단 시각보다 늦지 않다.
    assert!(again.iter().all(|f| f.known_at <= t(1_000)));
}

// 검증: V36, M2
#[test]
fn 판단_전_매매나_없는_매매_자료에는_붙이지_않는다() {
    let (_dir, _path, store) = temp_store();
    let judged = judged_trade(&store, 1_000);
    let discussing = store
        .create_trade(NewTrade {
            symbol: "000660".to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap();
    let ok = store
        .save_facts(&[fact("005930", number("1"), 10)])
        .unwrap()[0]
        .id;

    assert!(matches!(
        store.attach_trade_facts(discussing.id, &[ok]),
        Err(StoreError::TradeNotJudged { id, state })
            if id == discussing.id && state == TradeState::Discussing
    ));
    assert!(store.trade_facts(discussing.id).unwrap().is_empty());

    let missing_trade = TradeId(999);
    assert!(matches!(
        store.attach_trade_facts(missing_trade, &[ok]),
        Err(StoreError::TradeNotFound(id)) if id == missing_trade
    ));
    assert!(matches!(
        store.trade_facts(missing_trade),
        Err(StoreError::TradeNotFound(id)) if id == missing_trade
    ));

    assert!(matches!(
        store.attach_trade_facts(judged, &[ok, FactId(77)]),
        Err(StoreError::FactNotFound(id)) if id == FactId(77)
    ));
    assert!(store.trade_facts(judged).unwrap().is_empty());
}

// 검증: V36, C11
#[test]
fn 근거_자료는_sql로도_판단_시각보다_늦을_수_없다() {
    let (_dir, path, store) = temp_store();
    let judged = judged_trade(&store, 1_000);
    let discussing = store
        .create_trade(NewTrade {
            symbol: "000660".to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap();
    let saved = store
        .save_facts(&[
            fact("005930", number("1"), 900),
            fact("005930", number("2"), 1_001),
        ])
        .unwrap();
    store.attach_trade_facts(judged, &[saved[0].id]).unwrap();
    let before_links = store.trade_facts(judged).unwrap();
    drop(store);

    let (judged_id, discussing_id) = (judged.0, discussing.id.0);
    let (early_id, late_id) = (saved[0].id.0, saved[1].id.0);
    let late_known = t(1_001).0;
    let conn = rusqlite::Connection::open(&path).unwrap();
    let attempts = [
        // 판단 시각보다 늦은 자료를 붙이기.
        format!("INSERT INTO trade_facts (trade_id, fact_id) VALUES ({judged_id}, {late_id})"),
        // 판단 전 매매에 붙이기.
        format!("INSERT INTO trade_facts (trade_id, fact_id) VALUES ({discussing_id}, {early_id})"),
        // 붙은 자료의 알게 된 시각을 뒤로 미루기.
        format!("UPDATE facts SET known_at = {late_known} WHERE id = {early_id}"),
        // 자료는 지울 수 없다.
        format!("DELETE FROM facts WHERE id = {late_id}"),
    ];
    for sql in &attempts {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.trade_facts(judged).unwrap(), before_links);
    assert!(store.trade_facts(discussing.id).unwrap().is_empty());
    assert_eq!(store.get_fact(saved[0].id).unwrap(), saved[0]);
    assert_eq!(store.get_fact(saved[1].id).unwrap(), saved[1]);
}

// 검증: C11, V36
#[test]
fn 다시_열어도_자료와_붙인_근거가_그대로이고_번호가_이어진다() {
    let (_dir, path, store) = temp_store();
    let trade = judged_trade(&store, 1_000);
    let saved = store
        .save_facts(&[fact("005930", number("7.5"), 500)])
        .unwrap();
    store.attach_trade_facts(trade, &[saved[0].id]).unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_fact(saved[0].id).unwrap(), saved[0]);
    assert_eq!(store.trade_facts(trade).unwrap(), saved);
    assert_eq!(store.facts_known_by("005930", t(500)).unwrap(), saved);
    let next = store
        .save_facts(&[fact("005930", number("8"), 600)])
        .unwrap();
    assert_eq!(next[0].id, FactId(2));
}

// ───────────── 스키마 버전 6 이전 (D6) ─────────────

// 검증: D6, C11, V36
#[test]
fn 스키마_버전은_6이고_새_저장소의_새_표는_비어_있고_바로_쓸_수_있다() {
    assert_eq!(SCHEMA_VERSION, 8);
    let (_dir, path, store) = temp_store();
    assert_eq!(store.info().unwrap().schema_version, 8);
    assert_new_tables_empty_and_usable(&store, "새 파일");
    drop(store);
    assert_eq!(raw_user_version(&path), 8);
}

// 검증: D6, C11, V36
#[test]
fn 옛_파일을_열면_지우지_않고_6으로_올라가고_새_표를_바로_쓸_수_있다() {
    for name in FIXTURES {
        let (_dir, path) = copy_of(name);
        let before_tables = tables(&path);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.info().unwrap().schema_version, 8, "{name}");
        assert_new_tables_empty_and_usable(&store, name);
        drop(store);
        assert_eq!(raw_user_version(&path), 8, "{name}");
        let after_tables = tables(&path);
        for table in &before_tables {
            assert!(after_tables.contains(table), "{name}: {table}");
        }
    }
}

// 검증: D6, M2
#[test]
fn 버전_5_파일을_6으로_올려도_있던_줄이_글자까지_그대로이고_9차_자료가_읽힌다() {
    let (_dir, path) = copy_of("v5.db");
    assert_eq!(raw_user_version(&path), 5);
    let names = tables(&path);
    let before = dump(&path, &names);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 8);
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    let events = store.read_after(0, 100).unwrap();
    assert_eq!(events.len(), 14);
    assert_eq!(events[13].kind, "chat");
    assert_eq!(events[13].payload, json!({ "v5": true }));
    assert_eq!(
        store.get_template(TemplateId(1)).unwrap().value,
        json!({ "R%": "1", "style": "swing" })
    );
    assert_eq!(
        store.get_approval(ApprovalId(1)).unwrap().fingerprint,
        "sha256:v5"
    );
    assert_eq!(
        store.latest_paper_snapshot().unwrap().unwrap().state,
        json!({ "cash": "1000000", "v": 5 })
    );
    let access_key = store.access_key().unwrap().unwrap();
    assert_eq!(access_key.generation, 1);
    assert_eq!(access_key.sha256, [7u8; 32]);
    assert_eq!(store.team_history().unwrap().len(), 4);
    assert_eq!(store.access_log().unwrap().len(), 3);
    // 번호는 이어진다.
    assert_eq!(store.append("chat", &json!({ "n": 15 })).unwrap(), 15);
    drop(store);

    // 이벤트 한 줄을 더 쌓았으므로 이벤트 표를 빼고 비교한다. 이벤트 표의 앞 14줄은 그대로다.
    let after = dump(&path, &names);
    for ((name, rows_before), (_, rows_after)) in before.iter().zip(after.iter()) {
        if name == "events" {
            assert_eq!(rows_after[..rows_before.len()], rows_before[..], "{name}");
            assert_eq!(rows_after.len(), rows_before.len() + 1, "{name}");
        } else {
            assert_eq!(rows_after, rows_before, "{name}");
        }
    }
}

// 검증: D6, C11
#[test]
fn 올린_파일을_다시_열어도_6이고_새_표의_자료와_보호가_그대로다() {
    let (_dir, path) = copy_of("v5.db");
    let store = Store::open(&path).unwrap();
    let session = store.open_session("시세 담당", "팀", t(1)).unwrap();
    store
        .append_messages(session.id, &[msg(MessageRole::User, "안녕", 2)])
        .unwrap();
    let running = key("daily-review", 1_000);
    store.open_trigger_receipt(&running, t(1_001)).unwrap();
    let trade = judged_trade(&store, 1_000);
    let saved = store
        .save_facts(&[
            fact("005930", number("1"), 900),
            fact("005930", number("2"), 1_001),
        ])
        .unwrap();
    store.attach_trade_facts(trade, &[saved[0].id]).unwrap();
    drop(store);

    let names = tables(&path);
    let before = dump(&path, &names);
    for _ in 0..2 {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.info().unwrap().schema_version, 8);
        assert_eq!(store.session_messages(session.id).unwrap().len(), 1);
        drop(store);
        // 이전을 되풀이하지 않는다: 어떤 줄도 바뀌지 않았다.
        assert_eq!(dump(&path, &names), before);
    }

    let conn = rusqlite::Connection::open(&path).unwrap();
    let late_id = saved[1].id.0;
    for sql in [
        "UPDATE trigger_receipts SET status = 'bogus'".to_string(),
        format!(
            "INSERT INTO trade_facts (trade_id, fact_id) VALUES ({}, {late_id})",
            trade.0
        ),
    ] {
        assert!(conn.execute(&sql, []).is_err(), "{sql}");
    }
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.trade_facts(trade).unwrap(), vec![saved[0].clone()]);
    assert_eq!(
        store.trigger_receipts().unwrap()[0].status,
        TriggerReceiptStatus::Running
    );
}
