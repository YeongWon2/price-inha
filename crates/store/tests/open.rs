#![cfg(test)]
//! 열기와 이전 (D6), 다시 열기 (V6), 비밀 없음.

mod common;

use std::fs;

use common::{t, temp_store};
use pi_store::{
    AccountKind, Amount, Approval, Judgment, NewAccess, NewTrade, Rationale, RoleScore,
    SCHEMA_VERSION, SourceRef, Store, StoreError, TeamCause, TeamChange, TeamState, TradeState,
};
use serde_json::json;

fn team_change(to: TeamState, cause: TeamCause, at: i64) -> TeamChange {
    TeamChange {
        to,
        at: t(at),
        cause,
        detail: format!("{cause:?}"),
        owner_approved: false,
    }
}

fn judgment() -> Judgment {
    Judgment {
        decided_at: t(100),
        rationale: Rationale {
            hypothesis: Some("눌림 뒤 되돌림".to_string()),
            trigger: Some("거래량 급증".to_string()),
            entry: Some(Amount(71_500)),
            stop: Some(Amount(69_000)),
            target: Some(Amount(78_000)),
            expected_value: Some(Amount(1_200)),
        },
        sources: vec![SourceRef {
            name: "일봉 시세".to_string(),
            known_at: t(90),
        }],
        scores: vec![RoleScore {
            role: "시세 담당".to_string(),
            score: 70,
            reason: "추세".to_string(),
        }],
        consensus_score: 70,
    }
}

/// 장 중의 모습을 만든다: 운용 중, 보유 종목 둘(하나는 손절값을 좁혔다), 논의중 하나,
/// 이벤트 셋, 접속 기록 하나.
fn fill_mid_session(store: &Store) {
    store
        .change_team_state(team_change(
            TeamState::Operating,
            TeamCause::TradingStart,
            1,
        ))
        .unwrap();
    for (symbol, account) in [("005930", AccountKind::Paper), ("AAPL", AccountKind::Live)] {
        let id = store
            .create_trade(NewTrade {
                symbol: symbol.to_string(),
                account,
                at: t(10),
            })
            .unwrap()
            .id;
        store.judge_trade(id, judgment()).unwrap();
        store
            .transition_trade(id, TradeState::AwaitingApproval, t(110))
            .unwrap();
        store
            .set_trade_approval(
                id,
                Approval {
                    number: id.0 + 40,
                    content_fingerprint: format!("sha256:{symbol}"),
                },
                t(120),
            )
            .unwrap();
        store
            .transition_trade(id, TradeState::Ordered, t(130))
            .unwrap();
        store
            .transition_trade(id, TradeState::Holding, t(140))
            .unwrap();
        if symbol == "AAPL" {
            store.update_stop(id, Amount(70_250), t(150)).unwrap();
        }
    }
    store
        .create_trade(NewTrade {
            symbol: "000660".to_string(),
            account: AccountKind::Paper,
            at: t(160),
        })
        .unwrap();
    for n in 1..=3 {
        store
            .append("chat", &json!({ "n": n, "text": "장 중" }))
            .unwrap();
    }
    store
        .record_access(NewAccess {
            at: t(170),
            origin: "127.0.0.1".to_string(),
            action: "들어옴".to_string(),
        })
        .unwrap();
}

// 검증: D6
#[test]
fn 없는_파일을_열면_sqlite_파일_하나가_생긴다() {
    let (_dir, path, _store) = temp_store();
    assert!(path.is_file());
    let head = fs::read(&path).unwrap();
    assert!(head.starts_with(b"SQLite format 3\0"));
}

// 검증: D6
#[test]
fn 열면_스키마가_최신_버전이고_wal과_외래_키가_켜져_있다() {
    let (_dir, _path, store) = temp_store();
    let info = store.info().unwrap();
    assert_eq!(info.schema_version, SCHEMA_VERSION);
    assert_eq!(info.journal_mode, "wal");
    assert!(info.foreign_keys);
}

// 검증: D6
#[test]
fn wal은_파일에_남아_다른_연결로_열어도_wal이다() {
    let (_dir, path, store) = temp_store();
    drop(store);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

// 검증: D6
#[test]
fn 다시_열어도_스키마_버전은_그대로이고_이전을_되풀이하지_않는다() {
    let (_dir, path, store) = temp_store();
    fill_mid_session(&store);
    drop(store);
    for _ in 0..3 {
        let store = Store::open(&path).unwrap();
        let info = store.info().unwrap();
        assert_eq!(info.schema_version, SCHEMA_VERSION);
        assert_eq!(info.journal_mode, "wal");
        assert!(info.foreign_keys);
        // 이전이 다시 돌아 자료를 지우거나 겹쳐 넣지 않았다.
        assert_eq!(store.team_history().unwrap().len(), 1);
        assert_eq!(store.trades_in_state(TradeState::Holding).unwrap().len(), 2);
        assert_eq!(store.read_after(0, 100).unwrap().len(), 3);
        assert_eq!(store.access_log().unwrap().len(), 1);
    }
}

// 검증: D6
#[test]
fn sqlite_파일이_아니면_패닉하지_않고_오류를_준다() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-a-db.sqlite");
    fs::write(&path, "이것은 데이터베이스가 아니다. ".repeat(200)).unwrap();
    let result: Result<Store, StoreError> = Store::open(&path);
    assert!(result.is_err());
    // 열지 못한 파일을 덮어쓰지 않는다.
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .starts_with("이것은 데이터베이스가 아니다.")
    );
}

// 검증: D6
#[test]
fn 폴더를_파일로_열면_오류를_준다() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Store::open(dir.path()).is_err());
}

// 검증: V6, D6
#[test]
fn 껐다_켜면_팀_상태와_보유_종목과_기록이_그대로_이어진다() {
    let (_dir, path, store) = temp_store();
    fill_mid_session(&store);

    let team = store.team_state().unwrap();
    let history = store.team_history().unwrap();
    let holding = store.trades_in_state(TradeState::Holding).unwrap();
    let discussing = store.trades_in_state(TradeState::Discussing).unwrap();
    let events = store.read_after(0, 100).unwrap();
    let access = store.access_log().unwrap();
    assert_eq!(team, TeamState::Operating);
    assert_eq!(holding.len(), 2);
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.team_state().unwrap(), team);
    assert_eq!(store.team_history().unwrap(), history);
    // 보유 종목: 근거 · 좁힌 손절값 · 계좌 종류 · 점수 · 승인까지 모두 같다.
    let reopened = store.trades_in_state(TradeState::Holding).unwrap();
    assert_eq!(reopened, holding);
    assert_eq!(reopened[0].account, AccountKind::Paper);
    assert_eq!(reopened[0].rationale.stop, Some(Amount(69_000)));
    assert_eq!(reopened[1].account, AccountKind::Live);
    assert_eq!(reopened[1].rationale.stop, Some(Amount(70_250)));
    assert!(reopened.iter().all(|x| x.approval.is_some()));
    assert_eq!(
        store.trades_in_state(TradeState::Discussing).unwrap(),
        discussing
    );
    assert_eq!(store.read_after(0, 100).unwrap(), events);
    assert_eq!(store.access_log().unwrap(), access);
    for trade in &holding {
        assert_eq!(&store.get_trade(trade.id).unwrap(), trade);
    }
}

// 검증: V6
#[test]
fn 껐다_켠_뒤에도_하던_일을_이어서_할_수_있다() {
    let (_dir, path, store) = temp_store();
    fill_mid_session(&store);
    let holding = store.trades_in_state(TradeState::Holding).unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    // 번호는 처음부터 다시 세지 않고 이어진다.
    assert_eq!(
        store
            .append("chat", &json!({ "text": "다시 켜짐" }))
            .unwrap(),
        4
    );
    assert_eq!(
        store
            .record_access(NewAccess {
                at: t(300),
                origin: "127.0.0.1".to_string(),
                action: "다시 들어옴".to_string(),
            })
            .unwrap(),
        2
    );
    let rec = store
        .change_team_state(team_change(TeamState::Resting, TeamCause::MarketClose, 310))
        .unwrap();
    assert_eq!(rec.seq, 2);
    assert_eq!(rec.from, TeamState::Operating);

    // 보유 종목을 판다. 좁혀 둔 손절값의 규칙도 그대로다.
    let aapl = holding.iter().find(|x| x.symbol == "AAPL").unwrap();
    assert!(matches!(
        store.update_stop(aapl.id, Amount(70_000), t(320)),
        Err(StoreError::StopNotTightened { .. })
    ));
    let sold = store
        .transition_trade(aapl.id, TradeState::Sold, t(330))
        .unwrap();
    assert_eq!(sold.state, TradeState::Sold);
    assert_eq!(store.trades_in_state(TradeState::Holding).unwrap().len(), 1);

    // 새 매매 번호는 앞선 번호와 겹치지 않는다.
    let fresh = store
        .create_trade(NewTrade {
            symbol: "035420".to_string(),
            account: AccountKind::Paper,
            at: t(340),
        })
        .unwrap();
    assert!(holding.iter().all(|x| x.id != fresh.id));
}

// 검증: V6
#[test]
fn 멈춤_상태도_껐다_켜면_풀리지_않고_조건이_그대로_지켜진다() {
    let (_dir, path, store) = temp_store();
    store
        .change_team_state(team_change(
            TeamState::Operating,
            TeamCause::TradingStart,
            1,
        ))
        .unwrap();
    store
        .change_team_state(team_change(TeamState::FullStop, TeamCause::MaxLoss, 2))
        .unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.team_state().unwrap(), TeamState::FullStop);
    // 다시 켰다고 승인 없이 나갈 수 있게 되지 않는다.
    assert!(matches!(
        store.change_team_state(team_change(TeamState::Resting, TeamCause::OwnerRestart, 3)),
        Err(StoreError::TransitionDenied { .. })
    ));
    assert_eq!(store.team_state().unwrap(), TeamState::FullStop);
    assert_eq!(store.team_history().unwrap().len(), 2);
}

// 검증: V6
#[test]
fn 닫지_못하고_꺼져도_저장된_것은_남는다() {
    let (_dir, path, store) = temp_store();
    fill_mid_session(&store);
    let holding = store.trades_in_state(TradeState::Holding).unwrap();
    let events = store.read_after(0, 100).unwrap();
    // 갑자기 꺼진 것처럼 손잡이를 닫지 않고 버린다(연결을 닫는 코드가 돌지 않는다).
    std::mem::forget(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
    assert_eq!(store.trades_in_state(TradeState::Holding).unwrap(), holding);
    assert_eq!(store.read_after(0, 100).unwrap(), events);
    assert_eq!(store.access_log().unwrap().len(), 1);
}

// 검증: D6
#[test]
fn 복제한_손잡이는_같은_자료를_본다() {
    let (_dir, _path, store) = temp_store();
    let other = store.clone();
    store.append("chat", &json!({ "n": 1 })).unwrap();
    other
        .change_team_state(team_change(
            TeamState::Operating,
            TeamCause::TradingStart,
            1,
        ))
        .unwrap();
    assert_eq!(other.read_after(0, 10).unwrap().len(), 1);
    assert_eq!(store.team_state().unwrap(), TeamState::Operating);
}

// 검증: D6, M6
#[test]
fn 저장소에_비밀을_담는_칸이_없다() {
    let (_dir, path, store) = temp_store();
    fill_mid_session(&store);
    drop(store);

    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut names: Vec<String> = Vec::new();
    let mut tables = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap();
    let table_names: Vec<String> = tables
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!table_names.is_empty());
    for table in &table_names {
        names.push(table.clone());
        let mut columns = conn
            .prepare("SELECT name FROM pragma_table_info(?1)")
            .unwrap();
        let cols: Vec<String> = columns
            .query_map([table], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        names.extend(cols);
    }
    let banned = [
        "secret",
        "password",
        "passwd",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "credential",
        "비밀",
        "암호",
    ];
    for name in &names {
        let lower = name.to_lowercase();
        for word in banned {
            assert!(!lower.contains(word), "비밀을 담는 이름으로 보인다: {name}");
        }
    }
}
