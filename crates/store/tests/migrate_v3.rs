#![cfg(test)]
//! 3차 규칙 3 · 4: 스키마 버전 3. 버전 1 · 2 파일을 열면 3으로 올라가고 자료가 그대로다.
//! (4차: 최신 스키마 버전이 4가 되어, 열고 난 뒤의 버전을 단언하는 곳만 4로 바꿨다.)
//!
//! `tests/fixtures/v2.db`는 3차를 시작하기 전의 코드(스키마 버전 2)로 만든 저장소 파일이다.
//! WAL을 체크포인트해 파일 하나로 두었다. 테스트는 그 파일을 임시 폴더에 복사해서 연다.
//! (버전 1 파일 `v1.db`의 자료는 `migrate.rs`가 본다. 여기서는 이벤트 시각만 본다.)
//!
//! `v2.db`에 든 것:
//! - 투자 팀 기록 3줄. 마지막 상태는 운용 중.
//! - 매매 3건: 1 논의중 · 2 막힘(승인 41, 승인 거절) · 3 승인대기(승인 77).
//! - 접속 기록 2줄.
//! - 이벤트 7개. 내용이 `{"at": 숫자, "payload": …}` 봉투 모양인 것은 2 · 3 · 7번이다.
//!   3 · 4번은 이름 규칙이 생기기 전의 옛 이름(`Old.Kind` · `quotes stale`)이다.
//!   5번은 `at`이 숫자가 아니고 6번은 `payload`가 없어서 봉투 모양이 아니다.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    AccessRecord, AccountKind, Amount, Approval, BlockKind, BlockReason, Event, EventRange,
    NewAccess, NewTrade, Rationale, RoleScore, SCHEMA_VERSION, SourceRef, Store, StoreError,
    TeamCause, TeamChange, TeamState, TeamStateRecord, TimedEvent, Trade, TradeId, TradeState,
    UnixMillis,
};
use serde_json::{Value, json};

const FIXTURE_V1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db");
const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");

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

fn all() -> EventRange {
    EventRange::default()
}

// ───────────── 버전 2 파일에 든 것 ─────────────

fn v2_team_history() -> Vec<TeamStateRecord> {
    let rows = [
        (
            TeamState::Resting,
            TeamState::Operating,
            TeamCause::TradingStart,
            "오늘은 매매한다",
        ),
        (
            TeamState::Operating,
            TeamState::Resting,
            TeamCause::MarketClose,
            "장 마감",
        ),
        (
            TeamState::Resting,
            TeamState::Operating,
            TeamCause::TradingStart,
            "다시 매매한다",
        ),
    ];
    rows.into_iter()
        .enumerate()
        .map(|(i, (from, to, cause, detail))| TeamStateRecord {
            seq: i as u64 + 1,
            at: t(i as i64 + 1),
            from,
            to,
            cause,
            detail: detail.to_string(),
            owner_approved: false,
        })
        .collect()
}

fn v2_judged(
    id: i64,
    symbol: &str,
    account: AccountKind,
    state: TradeState,
    updated_at: i64,
    approval: Approval,
    blocked: Option<BlockReason>,
) -> Trade {
    Trade {
        id: TradeId(id),
        symbol: symbol.to_string(),
        account,
        state,
        created_at: t(10),
        updated_at: t(updated_at),
        rationale: Rationale {
            hypothesis: Some("눌림 뒤 되돌림".to_string()),
            trigger: Some("거래량 급증".to_string()),
            entry: Some(Amount(71_500)),
            stop: Some(Amount(69_000)),
            target: Some(Amount(78_000)),
            // 2^53 + 1: 부동소수점을 거치면 달라지는 값.
            expected_value: Some(Amount(9_007_199_254_740_993)),
        },
        decided_at: Some(t(100)),
        sources: vec![SourceRef {
            name: "일봉 시세".to_string(),
            known_at: t(90),
        }],
        scores: vec![RoleScore {
            role: "시세 담당".to_string(),
            score: 72,
            reason: "추세가 살아 있다".to_string(),
        }],
        consensus_score: Some(64),
        approval: Some(approval),
        blocked,
    }
}

fn v2_trades() -> Vec<Trade> {
    vec![
        Trade {
            id: TradeId(1),
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            state: TradeState::Discussing,
            created_at: t(10),
            updated_at: t(10),
            rationale: Rationale::default(),
            decided_at: None,
            sources: Vec::new(),
            scores: Vec::new(),
            consensus_score: None,
            approval: None,
            blocked: None,
        },
        v2_judged(
            2,
            "AAPL",
            AccountKind::Live,
            TradeState::Blocked,
            130,
            Approval {
                number: 41,
                content_fingerprint: "sha256:aapl".to_string(),
            },
            Some(BlockReason {
                kind: BlockKind::ApprovalRejected,
                detail: "내가 거절했다".to_string(),
            }),
        ),
        v2_judged(
            3,
            "051910",
            AccountKind::Paper,
            TradeState::AwaitingApproval,
            220,
            Approval {
                number: 77,
                content_fingerprint: "sha256:lgchem".to_string(),
            },
            None,
        ),
    ]
}

/// 버전 2 파일의 이벤트: (종류, 이전한 뒤의 시각, 내용). 번호는 순서대로 1부터다.
fn v2_event_rows() -> Vec<(&'static str, Option<UnixMillis>, Value)> {
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
    ]
}

fn v2_events() -> Vec<Event> {
    v2_event_rows()
        .into_iter()
        .enumerate()
        .map(|(i, (kind, _at, payload))| Event {
            seq: i as u64 + 1,
            kind: kind.to_string(),
            payload,
        })
        .collect()
}

fn v2_timed_events() -> Vec<TimedEvent> {
    v2_event_rows()
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

fn v2_access_log() -> Vec<AccessRecord> {
    vec![
        AccessRecord {
            seq: 1,
            at: t(600),
            origin: "127.0.0.1".to_string(),
            action: "들어옴".to_string(),
        },
        AccessRecord {
            seq: 2,
            at: t(610),
            origin: "192.168.0.7".to_string(),
            action: "매매 목록 봄".to_string(),
        },
    ]
}

/// 열린 저장소의 자료가 버전 2 파일에 든 것과 같은지 본다(3차 전부터 있던 읽기만 쓴다).
fn assert_v2_data(store: &Store) {
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
    assert_eq!(store.team_history().unwrap(), v2_team_history());
    for want in v2_trades() {
        assert_eq!(store.get_trade(want.id).unwrap(), want);
    }
    for state in TradeState::ALL {
        let want: Vec<TradeId> = v2_trades()
            .into_iter()
            .filter(|x| x.state == state)
            .map(|x| x.id)
            .collect();
        let got: Vec<TradeId> = store
            .trades_in_state(state)
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect();
        assert_eq!(got, want, "{state:?}");
    }
    assert!(matches!(
        store.get_trade(TradeId(4)),
        Err(StoreError::TradeNotFound(_))
    ));
    // 이벤트의 내용은 봉투 모양이던 줄도 그대로다.
    assert_eq!(store.read_after(0, 100).unwrap(), v2_events());
    assert_eq!(store.read_after(5, 100).unwrap(), v2_events()[5..]);
    assert_eq!(store.access_log().unwrap(), v2_access_log());
}

// 검증: D6, V6, M1, M2, M5, C2, E3, M6
#[test]
fn 버전_2_파일을_열면_최신_버전으로_올라가고_자료가_그대로다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    // 복사한 파일은 정말 버전 2다.
    assert_eq!(raw_user_version(&path), 2);

    let store = Store::open(&path).unwrap();
    let info = store.info().unwrap();
    assert_eq!(info.schema_version, SCHEMA_VERSION);
    assert_eq!(info.journal_mode, "wal");
    assert!(info.foreign_keys);
    assert_v2_data(&store);
    drop(store);
    assert_eq!(raw_user_version(&path), i64::from(SCHEMA_VERSION));
}

// 검증: D6, V6, C2, E3
#[test]
fn 봉투_모양이던_줄의_시각이_이벤트_시각으로_나오고_내용은_그대로다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();

    let got = store.read_events(all(), 100).unwrap();
    // 2 · 3 · 7번은 내용의 `at`이 시각이 되고, 봉투 모양이 아닌 1 · 4 · 5 · 6번은 시각이 없다.
    assert_eq!(
        got.iter().map(|e| e.at).collect::<Vec<_>>(),
        vec![
            None,
            Some(t(1000)),
            Some(t(1010)),
            None,
            None,
            None,
            Some(t(990)),
        ]
    );
    assert_eq!(got, v2_timed_events());
    // 내용은 건드리지 않았다.
    assert_eq!(store.read_after(0, 100).unwrap(), v2_events());
}

// 검증: D6, V6, C2, E3
#[test]
fn 올린_버전_2_파일을_다시_열어도_최신_버전이고_자료와_시각이_그대로다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    drop(Store::open(&path).unwrap());
    for _ in 0..3 {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
        assert_v2_data(&store);
        assert_eq!(store.read_events(all(), 100).unwrap(), v2_timed_events());
        drop(store);
        assert_eq!(raw_user_version(&path), i64::from(SCHEMA_VERSION));
    }
}

// 검증: D6, V6
#[test]
fn 버전_2_파일을_닫지_못하고_꺼져도_올린_것과_자료가_남는다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    std::mem::forget(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_v2_data(&store);
    assert_eq!(store.read_events(all(), 100).unwrap(), v2_timed_events());
}

// 검증: D6, C2, E3, M6, M1, M2
#[test]
fn 버전_2_파일을_올린_뒤에도_번호가_이어지고_새_이벤트에_시각이_남는다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);

    assert_eq!(
        store.append("chat", &json!({ "text": "올린 뒤" })).unwrap(),
        8
    );
    assert_eq!(
        store
            .append_at("chat", &json!({ "text": "시각과 함께" }), t(2000))
            .unwrap(),
        9
    );
    assert_eq!(
        store
            .record_access(NewAccess {
                at: t(700),
                origin: "127.0.0.1".to_string(),
                action: "다시 들어옴".to_string(),
            })
            .unwrap(),
        3
    );
    let rec = store
        .change_team_state(TeamChange {
            to: TeamState::Resting,
            at: t(710),
            cause: TeamCause::MarketClose,
            detail: "장 마감".to_string(),
            owner_approved: false,
        })
        .unwrap();
    assert_eq!(rec.seq, 4);
    let fresh = store
        .create_trade(NewTrade {
            symbol: "NVDA".to_string(),
            account: AccountKind::Paper,
            at: t(720),
        })
        .unwrap();
    assert_eq!(fresh.id, TradeId(4));
    drop(store);

    let store = Store::open(&path).unwrap();
    let events = store.read_events(all(), 100).unwrap();
    assert_eq!(events[..7], v2_timed_events()[..]);
    assert_eq!(
        events[7..],
        [
            TimedEvent {
                seq: 8,
                kind: "chat".to_string(),
                at: None,
                payload: json!({ "text": "올린 뒤" }),
            },
            TimedEvent {
                seq: 9,
                kind: "chat".to_string(),
                at: Some(t(2000)),
                payload: json!({ "text": "시각과 함께" }),
            },
        ]
    );
    assert_eq!(store.access_log().unwrap()[..2], v2_access_log()[..]);
    assert_eq!(store.team_history().unwrap()[..3], v2_team_history()[..]);
}

// 검증: D6, C2, E3
#[test]
fn 올린_파일의_이벤트를_옮겨_적은_시각으로_범위를_정해_읽는다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();
    store
        .append_at("chat", &json!({ "text": "새 줄" }), t(1005))
        .unwrap();
    let seqs = |range: EventRange| -> Vec<u64> {
        store
            .read_events(range, 100)
            .unwrap()
            .into_iter()
            .map(|e| e.seq)
            .collect()
    };
    // 시각: 2번 1000 · 3번 1010 · 7번 990 · 8번(새 줄) 1005.
    assert_eq!(
        seqs(EventRange {
            after_at: Some(t(995)),
            ..all()
        }),
        vec![2, 3, 8]
    );
    assert_eq!(
        seqs(EventRange {
            before_at: Some(t(1000)),
            ..all()
        }),
        vec![7]
    );
    assert_eq!(
        seqs(EventRange {
            after_seq: 2,
            after_at: Some(t(980)),
            before_at: Some(t(1010)),
        }),
        vec![7, 8]
    );
}

// 검증: D6, C2, E3
#[test]
fn 옛_이름의_줄은_그대로_읽히지만_그_이름으로_새로_쌓을_수는_없다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();

    let kinds: Vec<String> = store
        .read_after(0, 100)
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds[2], "Old.Kind");
    assert_eq!(kinds[3], "quotes stale");

    for old in ["Old.Kind", "quotes stale"] {
        match store.append(old, &json!({ "again": true })) {
            Err(StoreError::InvalidEventKind { kind }) => assert_eq!(kind, old),
            other => panic!("{old:?}은 이름 규칙에 어긋난다: {other:?}"),
        }
        match store.append_at(old, &json!({ "again": true }), t(3000)) {
            Err(StoreError::InvalidEventKind { kind }) => assert_eq!(kind, old),
            other => panic!("{old:?}은 이름 규칙에 어긋난다: {other:?}"),
        }
    }
    // 거부된 것은 번호를 쓰지 않았고 앞선 줄은 그대로다.
    assert_eq!(store.append("chat", &json!({ "n": 8 })).unwrap(), 8);
    assert_eq!(store.read_after(0, 7).unwrap(), v2_events());
}

// 검증: D6, M5
#[test]
fn 버전_2_파일을_올린_뒤에도_쓴_승인_번호는_다시_쓰지_못한다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    for number in [41, 77] {
        match store.set_trade_approval(
            TradeId(1),
            Approval {
                number,
                content_fingerprint: "sha256:reuse".to_string(),
            },
            t(910),
        ) {
            Err(StoreError::ApprovalNumberReused { number: n }) => assert_eq!(n, number),
            other => panic!("{number}번은 이미 쓴 승인 번호다: {other:?}"),
        }
    }
    assert_v2_data(&store);
}

// 검증: D6, V6, C2, E3
#[test]
fn 버전_1_파일의_이벤트는_최신_버전으로_올린_뒤_시각이_없음으로_읽힌다() {
    let (_dir, path) = copy_of(FIXTURE_V1);
    assert_eq!(raw_user_version(&path), 1);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);

    let plain = store.read_after(0, 100).unwrap();
    assert_eq!(plain.len(), 3);
    let with_time = store.read_events(all(), 100).unwrap();
    // 버전 1 파일에는 봉투 모양의 내용이 없다: 모두 시각이 없고 내용은 그대로다.
    assert_eq!(
        with_time,
        plain
            .into_iter()
            .map(|e| TimedEvent {
                seq: e.seq,
                kind: e.kind,
                at: None,
                payload: e.payload,
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        store.append_at("chat", &json!({ "n": 4 }), t(1)).unwrap(),
        4
    );
    drop(store);
    assert_eq!(raw_user_version(&path), i64::from(SCHEMA_VERSION));
}

// ───────────── 3차 규칙 4: 기록 보호는 그대로다 ─────────────

/// 공개 API로 읽은 기록 전체(이벤트는 시각까지).
#[derive(Debug, PartialEq)]
struct Records {
    team_state: TeamState,
    team_history: Vec<TeamStateRecord>,
    events: Vec<Event>,
    timed_events: Vec<TimedEvent>,
    access_log: Vec<AccessRecord>,
}

fn read_records(path: &Path) -> Records {
    let store = Store::open(path).unwrap();
    Records {
        team_state: store.team_state().unwrap(),
        team_history: store.team_history().unwrap(),
        events: store.read_after(0, 1_000).unwrap(),
        timed_events: store.read_events(all(), 1_000).unwrap(),
        access_log: store.access_log().unwrap(),
    }
}

/// SQL을 파일에 직접 넣어 본다. 저장소가 막아서 거절되거나(제약 오류) 조용히 무시되는 것은 괜찮다.
/// 그 밖의 오류는 시험하는 SQL 자체가 틀린 것이므로 테스트를 멈춘다.
fn try_sql(path: &Path, sql: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    match conn.execute(sql, []) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation => {}
        Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
    }
}

/// 이름이 `event`로 시작하는 표와 그 칸들. 시각을 어느 표 어느 칸에 두든 모두 건드려 보기 위해서다.
fn event_tables(path: &Path) -> Vec<(String, Vec<String>)> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'event%'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let columns: Vec<String> = conn
                .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (table, columns)
        })
        .collect()
}

/// 버전 4 파일의 기록 표에 고치기 · 지우기 · 다시 넣기를 해 보고 공개 API로 읽은 것이 그대로인지 본다.
fn assert_records_protected(path: &Path) {
    let before = read_records(path);
    assert!(before.events.len() >= 3);
    assert!(before.timed_events.iter().any(|e| e.at.is_some()));
    assert_eq!(raw_user_version(path), i64::from(SCHEMA_VERSION));

    let mut sqls: Vec<String> = Vec::new();
    let tables = event_tables(path);
    assert!(tables.iter().any(|(table, _)| table == "events"));
    for (table, columns) in &tables {
        // 줄을 가리키는 칸은 표의 첫 칸(이벤트 번호)이다.
        let key = &columns[0];
        // 칸마다(새 시각 칸도) 값을 바꾸거나 비워 본다.
        for column in columns {
            sqls.push(format!("UPDATE {table} SET {column} = 12345"));
            if column != key {
                // 번호 칸은 비울 수 없는 칸이라 SQLite가 다른 오류를 낸다. 번호는 아래에서 따로 바꿔 본다.
                sqls.push(format!("UPDATE {table} SET {column} = NULL"));
            }
            sqls.push(format!(
                "UPDATE OR REPLACE {table} SET {column} = 12345 WHERE {key} = 2"
            ));
        }
        sqls.push(format!("DELETE FROM {table} WHERE {key} = 2"));
        sqls.push(format!("DELETE FROM {table}"));
        // 있는 줄을 그대로, 또 다른 줄의 값으로 갈아 끼워 본다(모든 칸을 베껴서).
        let list = columns.join(", ");
        let rest = columns[1..].join(", ");
        sqls.push(format!(
            "INSERT OR REPLACE INTO {table} ({list}) SELECT {list} FROM {table} WHERE {key} = 2"
        ));
        sqls.push(format!(
            "INSERT OR REPLACE INTO {table} ({list}) SELECT 1, {rest} FROM {table} WHERE {key} = 2"
        ));
    }
    for fixed in [
        "INSERT OR REPLACE INTO events (seq, kind, payload) VALUES (1, 'forged', '{}')",
        "INSERT INTO events (seq, kind, payload) VALUES (2, 'forged', '{}')
             ON CONFLICT (seq) DO UPDATE SET kind = excluded.kind, payload = excluded.payload",
        "UPDATE events SET seq = seq + 10",
        "INSERT OR REPLACE INTO access_log (seq, at, origin, action) VALUES (1, 0, 'forged', 'forged')",
        "UPDATE access_log SET at = 0",
        "DELETE FROM access_log",
        "UPDATE team_history SET detail = '바꿔치기', owner_approved = 1",
        "DELETE FROM team_history",
        "INSERT OR REPLACE INTO team_history
             (seq, at, from_state, to_state, cause, detail, owner_approved)
         SELECT 1, at, from_state, to_state, cause, '바꿔치기', 1 FROM team_history WHERE seq = 2",
    ] {
        sqls.push(fixed.to_string());
    }
    for sql in sqls {
        try_sql(path, &sql);
        assert_eq!(read_records(path), before, "바뀌었다: {sql}");
    }
}

// 검증: D6, C2, E3, M6, M1
#[test]
fn 새로_만든_최신_버전_파일의_기록_표는_시각_칸까지_어떤_sql로도_바뀌지_않는다() {
    let (_dir, path, store) = temp_store();
    for (to, cause) in [
        (TeamState::Operating, TeamCause::TradingStart),
        (TeamState::Resting, TeamCause::MarketClose),
    ] {
        store
            .change_team_state(TeamChange {
                to,
                at: t(1),
                cause,
                detail: "원래 내용".to_string(),
                owner_approved: false,
            })
            .unwrap();
    }
    for n in 1..=4_i64 {
        if n % 2 == 0 {
            store
                .append_at("chat", &json!({ "n": n, "text": "원래 내용" }), t(n))
                .unwrap();
        } else {
            store
                .append("chat", &json!({ "n": n, "text": "원래 내용" }))
                .unwrap();
        }
        store
            .record_access(NewAccess {
                at: t(100 + n),
                origin: "127.0.0.1".to_string(),
                action: format!("{n}번째 접속"),
            })
            .unwrap();
    }
    drop(store);
    assert_records_protected(&path);
}

// 검증: D6, V6, C2, E3, M6, M1
#[test]
fn 버전_2에서_올린_파일의_기록_표도_시각_칸까지_어떤_sql로도_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    drop(Store::open(&path).unwrap());
    assert_records_protected(&path);

    let store = Store::open(&path).unwrap();
    assert_v2_data(&store);
    assert_eq!(store.read_events(all(), 100).unwrap(), v2_timed_events());
}

// 검증: D6, V6, C2, E3, M6, M1
#[test]
fn 버전_1에서_올린_파일의_기록_표도_시각_칸까지_어떤_sql로도_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V1);
    let store = Store::open(&path).unwrap();
    // 시각이 있는 줄도 하나 둔다.
    assert_eq!(
        store
            .append_at("chat", &json!({ "text": "올린 뒤" }), t(900))
            .unwrap(),
        4
    );
    drop(store);
    assert_records_protected(&path);
}

// ───────────── 4차: 이미 버전 3인 파일 ─────────────

/// 4차를 시작하기 전의 코드(스키마 버전 3)로 `v2.db`를 올린 뒤 이벤트 4개를 더 쌓은 파일
/// (`legacy_boundary.rs` 머리말).
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");

// 검증: D6, V6, C2, E3, M6, M1
#[test]
fn 버전_3에서_올린_파일의_기록_표도_시각_칸까지_어떤_sql로도_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    assert_eq!(raw_user_version(&path), 3);
    drop(Store::open(&path).unwrap());
    assert_records_protected(&path);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.read_after(0, 100).unwrap().len(), 11);
    assert_eq!(store.read_after(0, 7).unwrap(), v2_events());
}
