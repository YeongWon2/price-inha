//! 승인 증표를 저장소의 승인 기록과 대조한다(2차).
//!
//! 증표([`ApprovalGrant`])는 누구나 만들 수 있으므로, `pi-approval`이 저장소에 남긴 승인 기록을
//! 그 번호로 읽어 이 매매의 승인인지 본다. 승인 모듈의 공개 읽기([`Approvals::get`])만 쓴다.

use std::sync::Arc;

use pi_approval::{
    ApprovalNumber, Approvals, OrderType as SubjectOrderType, Side as SubjectSide, Subject,
    WaitTimes, fingerprint,
};
use pi_market::Clock;
use pi_paper::{ApprovedOrder, OrderType, Side};
use pi_rules::ApprovalGrant;
use pi_store::{ApprovalStatus, Store, Trade, UnixMillis};
use rust_decimal::Decimal;

use crate::error::Error;

/// 승인한 주문 내용의 신호 id: `trade-<매매 번호>`(조립하는 쪽의 약속).
fn signal_id(trade: &Trade) -> String {
    format!("trade-{}", trade.id.0)
}

/// 승인 모듈에 줄 시계(주입한 시계를 그대로 읽는다).
struct MillisClock(Arc<dyn Clock>);

impl pi_approval::Clock for MillisClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

/// 저장소의 승인 기록을 읽는 것.
pub(crate) fn reader(store: Store, clock: Arc<dyn Clock>) -> Approvals {
    Approvals::new(store, Arc::new(MillisClock(clock)), WaitTimes::default())
}

/// 증표가 이 매매 · 이 주문 내용의 승인인가. 아니면 [`Error::ApprovalMismatch`].
///
/// 그 번호의 기록이 있고, 승인됨 · 소비됨이고, 지문이 증표 · 기록의 내용과 같고, 승인한 내용이
/// 이 매매(신호 id · 종목 · 계좌)와 주문(방향 · 수량 · 값 · 호가 유형)과 같아야 한다.
pub(crate) fn check(
    approvals: &Approvals,
    grant: &ApprovalGrant,
    trade: &Trade,
    order: &ApprovedOrder,
) -> Result<(), Error> {
    let found = match approvals.get(&ApprovalNumber::new(grant.id())) {
        Ok(found) => found,
        Err(pi_approval::Error::Store(e)) => return Err(Error::Store(e)),
        Err(_) => return Err(Error::ApprovalMismatch),
    };
    let Some(view) = found else {
        return Err(Error::ApprovalMismatch);
    };
    let usable = view.status == ApprovalStatus::Approved && view.consumed;
    let sealed = view.fingerprint.as_str() == grant.fingerprint()
        && fingerprint(&view.subject).as_str() == grant.fingerprint();
    let Subject::Order(subject) = &view.subject else {
        return Err(Error::ApprovalMismatch);
    };
    let side = match order.side {
        Side::Buy => SubjectSide::Buy,
        Side::Sell => SubjectSide::Sell,
    };
    let order_type = match order.order_type {
        OrderType::Limit => SubjectOrderType::Limit,
        OrderType::Market => SubjectOrderType::Market,
    };
    let same = subject.signal_id == signal_id(trade)
        && subject.symbol == trade.symbol
        && subject.account == trade.account
        && subject.side == side
        && subject.order_type == order_type
        && Decimal::from(subject.quantity) == order.quantity
        && Some(subject.price) == order.price;
    if usable && sealed && same {
        Ok(())
    } else {
        Err(Error::ApprovalMismatch)
    }
}
