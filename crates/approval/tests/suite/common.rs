//! 시험 도구: 임시 폴더의 실제 저장소, 가짜 시계, 대상 만들기.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use pi_approval::{
    Approvals, Ask, Clock, OrderSubject, OrderType, Side, SignalKind, Subject, WaitTimes,
};
use pi_store::{AccountKind, Store, TeamCause, TeamChange, TeamState, UnixMillis};
use rust_decimal::Decimal;
use tempfile::TempDir;

/// 시험의 시작 시각.
pub const T0: i64 = 1_800_000_000_000;
/// 1분(밀리초).
pub const MIN: i64 = 60_000;

/// 테스트가 정하는 시계.
pub struct FakeClock(AtomicI64);

impl FakeClock {
    pub fn at(ms: i64) -> Arc<FakeClock> {
        Arc::new(FakeClock(AtomicI64::new(ms)))
    }
    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

pub struct Env {
    /// 임시 폴더를 테스트 끝까지 살려 둔다(떨어지면 지워진다).
    pub _dir: TempDir,
    pub path: PathBuf,
    pub store: Store,
    pub clock: Arc<FakeClock>,
    pub approvals: Arc<Approvals>,
}

pub fn env() -> Env {
    env_with(WaitTimes::default())
}

pub fn env_with(waits: WaitTimes) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let store = Store::open(&path).unwrap();
    let clock = FakeClock::at(T0);
    let approvals = Arc::new(Approvals::new(store.clone(), clock.clone(), waits));
    Env {
        _dir: dir,
        path,
        store,
        clock,
        approvals,
    }
}

/// 같은 파일을 다시 열어 새 관리자를 만든다(다시 켜기).
pub fn reopen(path: &PathBuf, clock: Arc<FakeClock>) -> (Store, Approvals) {
    let store = Store::open(path).unwrap();
    let approvals = Approvals::new(store.clone(), clock, WaitTimes::default());
    (store, approvals)
}

pub fn order(signal_id: &str, side: Side, kind: SignalKind) -> OrderSubject {
    OrderSubject {
        account: AccountKind::Paper,
        symbol: "005930".to_string(),
        side,
        quantity: 10,
        price: Decimal::new(71_000, 0),
        order_type: OrderType::Limit,
        signal_id: signal_id.to_string(),
        signal_kind: kind,
        signal_valid_until: UnixMillis(T0 + 30 * MIN),
    }
}

pub fn buy(signal_id: &str) -> Subject {
    Subject::Order(order(signal_id, Side::Buy, SignalKind::Buy))
}

pub fn stop_loss(signal_id: &str) -> Subject {
    Subject::Order(order(signal_id, Side::Sell, SignalKind::StopLoss))
}

pub fn take_profit(signal_id: &str) -> Subject {
    Subject::Order(order(signal_id, Side::Sell, SignalKind::TakeProfit))
}

pub fn ask(subject: Subject) -> Ask {
    Ask {
        subject,
        reason: "금액 한도를 넘는 주문".to_string(),
    }
}

/// 쉬는 중 → 운용 중 → 전체 정지.
pub fn full_stop(store: &Store) {
    store
        .change_team_state(TeamChange {
            to: TeamState::Operating,
            at: UnixMillis(T0 - 10),
            cause: TeamCause::TradingStart,
            detail: "시험".to_string(),
            owner_approved: false,
        })
        .unwrap();
    store
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: UnixMillis(T0 - 5),
            cause: TeamCause::MaxLoss,
            detail: "시험".to_string(),
            owner_approved: false,
        })
        .unwrap();
}

/// 전체 정지 → 쉬는 중(사람이 다시 시작).
pub fn owner_restart(store: &Store) {
    store
        .change_team_state(TeamChange {
            to: TeamState::Resting,
            at: UnixMillis(T0 - 1),
            cause: TeamCause::OwnerRestart,
            detail: "시험".to_string(),
            owner_approved: true,
        })
        .unwrap();
}
