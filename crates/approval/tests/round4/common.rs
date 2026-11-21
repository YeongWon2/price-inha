//! 시험 도구: 임시 폴더의 실제 저장소, 가짜 시계, 대상 만들기.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use pi_approval::{
    Approvals, Ask, Clock, Notice, OrderSubject, OrderType, Requested, Side, SignalKind, Subject,
    WaitTimes,
};
use pi_store::{AccountKind, Store, UnixMillis};
use rust_decimal::Decimal;
use tempfile::TempDir;

/// 시험의 시작 시각.
pub const T0: i64 = 1_800_000_000_000;
/// 1분(밀리초).
pub const MIN: i64 = 60_000;
/// 시험 주문의 신호 유효 시간 끝.
pub const VALID_UNTIL: i64 = T0 + 30 * MIN;
/// 시험 주문의 수량.
pub const QTY: u64 = 10;

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
    pub clock: Arc<FakeClock>,
    pub approvals: Approvals,
}

pub fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let store = Store::open(&path).unwrap();
    let clock = FakeClock::at(T0);
    let approvals = Approvals::new(store, clock.clone(), WaitTimes::default());
    Env {
        _dir: dir,
        path,
        clock,
        approvals,
    }
}

/// 같은 파일을 다시 열어 새 관리자를 만든다(다시 켜기).
pub fn reopen(path: &PathBuf, clock: Arc<FakeClock>) -> Approvals {
    let store = Store::open(path).unwrap();
    Approvals::new(store, clock, WaitTimes::default())
}

pub fn order(signal_id: &str, side: Side, kind: SignalKind) -> Subject {
    Subject::Order(OrderSubject {
        account: AccountKind::Paper,
        symbol: "005930".to_string(),
        side,
        quantity: QTY,
        price: Decimal::new(71_000, 0),
        order_type: OrderType::Limit,
        signal_id: signal_id.to_string(),
        signal_kind: kind,
        signal_valid_until: UnixMillis(VALID_UNTIL),
    })
}

pub fn buy(signal_id: &str) -> Subject {
    order(signal_id, Side::Buy, SignalKind::Buy)
}

pub fn stop_loss(signal_id: &str) -> Subject {
    order(signal_id, Side::Sell, SignalKind::StopLoss)
}

pub fn take_profit(signal_id: &str) -> Subject {
    order(signal_id, Side::Sell, SignalKind::TakeProfit)
}

pub fn ask(subject: Subject) -> Ask {
    Ask {
        subject,
        reason: "금액 한도를 넘는 주문".to_string(),
    }
}

/// 요청을 만들고 알림 결과를 남긴다.
pub fn requested(e: &Env, subject: Subject, notice: Option<Notice>) -> Requested {
    let r = e.approvals.request(ask(subject)).unwrap();
    if let Some(n) = notice {
        e.approvals.record_notice(&r.number, n).unwrap();
    }
    r
}

/// 사는 주문(손절 1분 · 익절 5분 · 사기 5분)의 기다릴 시간이 지나는 시각으로 시계를 옮기고 닫는다.
pub fn close_due_at(e: &Env, at: i64) -> Vec<pi_approval::DueOutcome> {
    e.clock.set(at);
    e.approvals.expire_due(UnixMillis(at)).unwrap()
}
