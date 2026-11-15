#![cfg(test)]
//! 2차 규칙 6 · 3차 규칙 3: 스키마 버전. 버전 1 파일을 열면 최신(3차부터 3)으로 올라가고 자료가 그대로다.
//! 버전 2 파일과 3차에서 더한 것(이벤트 시각 칸)은 `migrate_v3.rs`에서 본다.
//!
//! `tests/fixtures/v1.db`는 2차를 시작하기 전의 코드(스키마 버전 1)로 만든 저장소 파일이다.
//! WAL을 체크포인트해 파일 하나로 두었다. 테스트는 그 파일을 임시 폴더에 복사해서 연다.
//!
//! 파일에 든 것:
//! - 투자 팀 기록 4줄. 마지막 상태는 하루 한도 멈춤.
//! - 매매 6건: 1 논의중 · 2 보유중(승인 41, 손절값 좁힘) · 3 팔림 · 4 막힘(버전 1이라 이유 없음) ·
//!   5 승인대기(승인 77) · 6 주문됨.
//! - 이벤트 3개, 접속 기록 2줄.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{t, temp_store};
use pi_store::{
    AccessRecord, AccountKind, Amount, Approval, BlockKind, BlockReason, Event, NewAccess,
    NewTrade, Rationale, RoleScore, SCHEMA_VERSION, SourceRef, Store, StoreError, TeamCause,
    TeamChange, TeamState, TeamStateRecord, Trade, TradeId, TradeState,
};
use serde_json::json;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db");

/// 버전 1 파일을 임시 폴더에 복사한다. 원본은 건드리지 않는다.
fn v1_copy() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(FIXTURE, &path).unwrap();
    (dir, path)
}

/// 파일에 적힌 스키마 버전을 저장소를 거치지 않고 읽는다.
fn raw_user_version(path: &PathBuf) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn full_rationale() -> Rationale {
    Rationale {
        hypothesis: Some("눌림 뒤 되돌림".to_string()),
        trigger: Some("거래량 급증".to_string()),
        entry: Some(Amount(71_500)),
        stop: Some(Amount(69_000)),
        target: Some(Amount(78_000)),
        // 2^53 + 1: 부동소수점을 거치면 달라지는 값.
        expected_value: Some(Amount(9_007_199_254_740_993)),
    }
}

/// 버전 1 파일의 판단된 매매 한 건(판단 시각 · 자료 · 점수는 모두 같은 틀이다).
fn judged(
    id: i64,
    symbol: &str,
    account: AccountKind,
    state: TradeState,
    updated_at: i64,
    rationale: Rationale,
    approval: Option<Approval>,
) -> Trade {
    Trade {
        id: TradeId(id),
        symbol: symbol.to_string(),
        account,
        state,
        created_at: t(10),
        updated_at: t(updated_at),
        rationale,
        decided_at: Some(t(100)),
        sources: vec![
            SourceRef {
                name: format!("{symbol} 일봉 시세"),
                known_at: t(90),
            },
            SourceRef {
                name: format!("{symbol} 공시"),
                known_at: t(100),
            },
        ],
        scores: vec![
            RoleScore {
                role: "시세 담당".to_string(),
                score: 72,
                reason: "추세가 살아 있다".to_string(),
            },
            RoleScore {
                role: "위험 담당".to_string(),
                score: 55,
                reason: "손절 폭이 넓다".to_string(),
            },
        ],
        consensus_score: Some(64),
        approval,
        blocked: None,
    }
}

/// 버전 1 파일에 든 매매 6건(번호 순서).
fn v1_trades() -> Vec<Trade> {
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
        judged(
            2,
            "AAPL",
            AccountKind::Live,
            TradeState::Holding,
            150,
            Rationale {
                stop: Some(Amount(70_250)),
                ..full_rationale()
            },
            Some(Approval {
                number: 41,
                content_fingerprint: "sha256:aapl".to_string(),
            }),
        ),
        judged(
            3,
            "000660",
            AccountKind::Paper,
            TradeState::Sold,
            230,
            full_rationale(),
            None,
        ),
        judged(
            4,
            "035420",
            AccountKind::Paper,
            TradeState::Blocked,
            310,
            Rationale::default(),
            None,
        ),
        judged(
            5,
            "051910",
            AccountKind::Paper,
            TradeState::AwaitingApproval,
            420,
            full_rationale(),
            Some(Approval {
                number: 77,
                content_fingerprint: "sha256:lgchem".to_string(),
            }),
        ),
        judged(
            6,
            "TSLA",
            AccountKind::Live,
            TradeState::Ordered,
            510,
            full_rationale(),
            None,
        ),
    ]
}

fn v1_team_history() -> Vec<TeamStateRecord> {
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
        (
            TeamState::Operating,
            TeamState::DailyLimitHalt,
            TeamCause::DailyLossLimit,
            "하루 손실 한도 도달",
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

fn v1_events() -> Vec<Event> {
    vec![
        Event {
            seq: 1,
            kind: "chat".to_string(),
            payload: json!({ "n": 1, "text": "장 시작" }),
        },
        Event {
            seq: 2,
            kind: "trade".to_string(),
            payload: json!({ "trade": 2, "amount": 9_007_199_254_740_993_i64 }),
        },
        Event {
            seq: 3,
            kind: "chat".to_string(),
            payload: json!({ "n": 3, "nested": { "list": [1, "둘", null] } }),
        },
    ]
}

fn v1_access_log() -> Vec<AccessRecord> {
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

/// 버전 1에서 이유 없이 막힌 매매(4번)의 `blocked`는 지시서가 정하지 않았다.
/// 그 칸만 빼고 견준다.
fn without_block_reason(trade: Trade) -> Trade {
    Trade {
        blocked: None,
        ..trade
    }
}

/// 열린 저장소의 자료가 버전 1 파일에 든 것과 같은지 본다.
fn assert_v1_data(store: &Store) {
    assert_eq!(store.team_state().unwrap(), TeamState::DailyLimitHalt);
    assert_eq!(store.team_history().unwrap(), v1_team_history());
    for want in v1_trades() {
        let got = store.get_trade(want.id).unwrap();
        if want.state == TradeState::Blocked {
            assert_eq!(without_block_reason(got), want);
        } else {
            // 막히지 않은 매매에는 막힌 이유가 없다.
            assert_eq!(got, want);
        }
    }
    for state in TradeState::ALL {
        let want: Vec<TradeId> = v1_trades()
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
        store.get_trade(TradeId(7)),
        Err(StoreError::TradeNotFound(_))
    ));
    assert_eq!(store.read_after(0, 100).unwrap(), v1_events());
    assert_eq!(store.read_after(2, 100).unwrap(), v1_events()[2..]);
    assert_eq!(store.access_log().unwrap(), v1_access_log());
}

// 검증: D6
#[test]
fn 새_저장소는_최신_스키마_버전으로_만들어진다() {
    assert_eq!(SCHEMA_VERSION, 5);
    let (_dir, path, store) = temp_store();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    drop(store);
    assert_eq!(raw_user_version(&path), i64::from(SCHEMA_VERSION));
}

// 검증: D6, V6, M1, M2, C2, E3, M6
#[test]
fn 버전_1_파일을_열면_최신_버전으로_올라가고_자료가_그대로다() {
    let (_dir, path) = v1_copy();
    // 복사한 파일은 정말 버전 1이다.
    assert_eq!(raw_user_version(&path), 1);

    let store = Store::open(&path).unwrap();
    let info = store.info().unwrap();
    assert_eq!(info.schema_version, SCHEMA_VERSION);
    assert_eq!(info.journal_mode, "wal");
    assert!(info.foreign_keys);
    assert_v1_data(&store);
    drop(store);
    assert_eq!(raw_user_version(&path), i64::from(SCHEMA_VERSION));
}

// 검증: D6, V6
#[test]
fn 올린_파일을_다시_열어도_최신_버전이고_이전을_되풀이하지_않는다() {
    let (_dir, path) = v1_copy();
    drop(Store::open(&path).unwrap());
    for _ in 0..3 {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
        assert_v1_data(&store);
        drop(store);
        assert_eq!(raw_user_version(&path), i64::from(SCHEMA_VERSION));
    }
}

// 검증: D6, V6
#[test]
fn 버전_1_파일을_닫지_못하고_꺼져도_올린_것과_자료가_남는다() {
    let (_dir, path) = v1_copy();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    std::mem::forget(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_v1_data(&store);
}

// 검증: D6, C2, E3, M6, M1, M2
#[test]
fn 올린_뒤에도_이벤트와_접속_기록과_팀_기록과_매매_번호가_이어진다() {
    let (_dir, path) = v1_copy();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, SCHEMA_VERSION);

    assert_eq!(
        store.append("chat", &json!({ "text": "올린 뒤" })).unwrap(),
        4
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
            cause: TeamCause::NextTradingDayStart,
            detail: "다음 거래일".to_string(),
            owner_approved: false,
        })
        .unwrap();
    assert_eq!(rec.seq, 5);
    assert_eq!(rec.from, TeamState::DailyLimitHalt);
    let fresh = store
        .create_trade(NewTrade {
            symbol: "NVDA".to_string(),
            account: AccountKind::Paper,
            at: t(720),
        })
        .unwrap();
    assert_eq!(fresh.id, TradeId(7));
    assert_eq!(fresh.blocked, None);

    // 앞선 자료는 그대로다.
    assert_eq!(store.read_after(0, 3).unwrap(), v1_events());
    assert_eq!(store.access_log().unwrap()[..2], v1_access_log()[..]);
    assert_eq!(store.team_history().unwrap()[..4], v1_team_history()[..]);
}

// 검증: D6, M1
#[test]
fn 올린_파일에서도_멈춘_동안_최대_손실이면_전체_정지가_된다() {
    let (_dir, path) = v1_copy();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.team_state().unwrap(), TeamState::DailyLimitHalt);
    let rec = store
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: t(700),
            cause: TeamCause::MaxLoss,
            detail: "멈춘 동안 최대 손실 도달".to_string(),
            owner_approved: false,
        })
        .unwrap();
    assert_eq!(rec.seq, 5);
    assert_eq!(
        (rec.from, rec.to),
        (TeamState::DailyLimitHalt, TeamState::FullStop)
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.team_state().unwrap(), TeamState::FullStop);
}

// 검증: D6, M2, M5, E12
#[test]
fn 올린_파일의_매매를_이유와_함께_막을_수_있고_다시_열어도_남는다() {
    let (_dir, path) = v1_copy();
    let store = Store::open(&path).unwrap();

    // 5번(승인대기)은 승인 거절로, 6번(주문됨)은 미체결 취소로 막는다.
    let rejected = BlockReason {
        kind: BlockKind::ApprovalRejected,
        detail: "내가 승인을 거절했다".to_string(),
    };
    let unfilled = BlockReason {
        kind: BlockKind::UnfilledCancelled,
        detail: "체결이 없어 주문을 취소했다".to_string(),
    };
    let five = store
        .block_trade(TradeId(5), rejected.clone(), t(800))
        .unwrap();
    let six = store
        .block_trade(TradeId(6), unfilled.clone(), t(810))
        .unwrap();
    assert_eq!(five.state, TradeState::Blocked);
    assert_eq!(five.blocked, Some(rejected));
    assert_eq!(six.state, TradeState::Blocked);
    assert_eq!(six.blocked, Some(unfilled));
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_trade(TradeId(5)).unwrap(), five);
    assert_eq!(store.get_trade(TradeId(6)).unwrap(), six);
    // 다른 매매는 그대로다.
    let two = store.get_trade(TradeId(2)).unwrap();
    assert_eq!(two, v1_trades()[1]);
    assert_eq!(
        store
            .trades_in_state(TradeState::Blocked)
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect::<Vec<_>>(),
        vec![TradeId(4), TradeId(5), TradeId(6)]
    );
}

// 검증: D6, M5
#[test]
fn 올린_파일에서도_승인은_주문_전에만_붙고_버전_1에서_쓴_번호를_다시_쓰지_못한다() {
    let (_dir, path) = v1_copy();
    let store = Store::open(&path).unwrap();
    let before = v1_trades();

    // 2번은 보유중이다: 승인을 바꿀 수 없다.
    assert!(matches!(
        store.set_trade_approval(
            TradeId(2),
            Approval {
                number: 500,
                content_fingerprint: "sha256:changed".to_string(),
            },
            t(900),
        ),
        Err(StoreError::ApprovalLocked {
            state: TradeState::Holding
        })
    ));
    // 3번(팔림)과 6번(주문됨)에는 새로 붙일 수 없다.
    for (id, state) in [(3, TradeState::Sold), (6, TradeState::Ordered)] {
        match store.set_trade_approval(
            TradeId(id),
            Approval {
                number: 500 + id,
                content_fingerprint: "sha256:late".to_string(),
            },
            t(900),
        ) {
            Err(StoreError::ApprovalLocked { state: s }) => assert_eq!(s, state),
            other => panic!("{state:?}의 매매에는 승인을 붙일 수 없다: {other:?}"),
        }
    }
    // 버전 1에서 2번 · 5번에 붙인 번호(41 · 77)는 다른 매매(1번, 논의중)에 붙일 수 없다.
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
    for want in [&before[0], &before[1], &before[2], &before[4], &before[5]] {
        assert_eq!(&store.get_trade(want.id).unwrap(), want);
    }
}

// 검증: D6, C2, E3, M6, M1
#[test]
fn 올린_파일의_기록_표도_같은_번호로_다시_넣어_바꿀_수_없다() {
    let (_dir, path) = v1_copy();
    drop(Store::open(&path).unwrap());

    let conn = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        "INSERT OR REPLACE INTO events (seq, kind, payload) VALUES (1, 'forged', '{}')",
        "INSERT OR REPLACE INTO access_log (seq, at, origin, action) VALUES (2, 0, 'forged', 'forged')",
        "INSERT OR REPLACE INTO team_history
             (seq, at, from_state, to_state, cause, detail, owner_approved)
         SELECT 4, at, from_state, to_state, cause, '바꿔치기', 1 FROM team_history WHERE seq = 2",
        "UPDATE events SET kind = 'forged' WHERE seq = 3",
        "DELETE FROM access_log WHERE seq = 1",
    ] {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation => {}
            Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
        }
    }
    drop(conn);

    let store = Store::open(&path).unwrap();
    assert_v1_data(&store);
}
