//! 승인 증표를 저장소의 승인 기록과 대조한다(2차).
//!
//! 증표([`ApprovalGrant`])는 누구나 만들 수 있으므로, `pi-approval`이 저장소에 남긴 승인 기록을
//! 그 번호로 읽어 이 매매의 승인인지 본다. 승인 모듈의 공개 읽기([`Approvals::get`])만 쓴다.

use std::sync::Arc;

use pi_approval::{
    ApprovalNumber, Approvals, DueAction, OrderSubject, OrderType as SubjectOrderType,
    Side as SubjectSide, Subject, WaitTimes, fingerprint,
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

/// 승인 기록을 읽다 난 오류: 저장소 오류는 그대로, 그 밖은 `mismatch`.
fn read_failed(error: pi_approval::Error, mismatch: Error) -> Error {
    match error {
        pi_approval::Error::Store(e) => Error::Store(e),
        _ => mismatch,
    }
}

/// 요청한 주문이 이 매매(신호 id · 종목 · 계좌)의 것이고 방향 · 호가 유형이 주문과 같은가.
/// 수량 · 값은 보지 않는다(길마다 다르게 본다).
fn same_target(subject: &OrderSubject, trade: &Trade, order: &ApprovedOrder) -> bool {
    let side = match order.side {
        Side::Buy => SubjectSide::Buy,
        Side::Sell => SubjectSide::Sell,
    };
    let order_type = match order.order_type {
        OrderType::Limit => SubjectOrderType::Limit,
        OrderType::Market => SubjectOrderType::Market,
    };
    subject.signal_id == signal_id(trade)
        && subject.symbol == trade.symbol
        && subject.account == trade.account
        && subject.side == side
        && subject.order_type == order_type
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
    let found = approvals
        .get(&ApprovalNumber::new(grant.id()))
        .map_err(|e| read_failed(e, Error::ApprovalMismatch))?;
    let Some(view) = found else {
        return Err(Error::ApprovalMismatch);
    };
    let usable = view.status == ApprovalStatus::Approved && view.consumed;
    let sealed = view.fingerprint.as_str() == grant.fingerprint()
        && fingerprint(&view.subject).as_str() == grant.fingerprint();
    let Subject::Order(subject) = &view.subject else {
        return Err(Error::ApprovalMismatch);
    };
    let same = same_target(subject, trade, order)
        && Decimal::from(subject.quantity) == order.quantity
        && Some(subject.price) == order.price;
    if usable && sealed && same {
        Ok(())
    } else {
        Err(Error::ApprovalMismatch)
    }
}

/// 그 번호가 이 매매의 "답 없음 · 줄여서 낼 것"이고 줄인 주문이 요청 안에 드는가(E22 · FN2 3b3).
/// 아니면 [`Error::UnansweredMismatch`]. 승인 기록은 읽기만 한다.
///
/// - 기록이 있고, 결과가 답 없음이고, 소비되지 않았다.
/// - 처리 기록이 없는 닫힌 요청 목록에 있고 할 일이 "줄여서 낸다"다(사는 요청 · 알림이 감(P20) ·
///   신호 유효 시간 안 · 처리 기록 없음).
/// - 요청한 주문이 이 매매의 사기이고 호가 유형이 주문과 같다. 값은 대조하지 않는다.
/// - `0 < 주문 수량 ≤ 요청 수량`, `주문 수량 × 주문 지정가 ≤ 요청 수량 × 요청 값`.
pub(crate) fn check_unanswered(
    approvals: &Approvals,
    number: &ApprovalNumber,
    trade: &Trade,
    order: &ApprovedOrder,
) -> Result<(), Error> {
    let mismatch = || Error::UnansweredMismatch;
    let view = approvals
        .get(number)
        .map_err(|e| read_failed(e, mismatch()))?
        .ok_or_else(mismatch)?;
    if view.status != ApprovalStatus::NoAnswer || view.consumed {
        return Err(mismatch());
    }
    let due = approvals
        .unhandled_due()
        .map_err(|e| read_failed(e, mismatch()))?;
    let to_reduce = due
        .iter()
        .any(|d| &d.number == number && d.action == DueAction::ReduceWithinLimit);
    let Subject::Order(subject) = &view.subject else {
        return Err(mismatch());
    };
    let buying = order.side == Side::Buy && same_target(subject, trade, order);
    if to_reduce && buying && within_request(subject, order) {
        Ok(())
    } else {
        Err(mismatch())
    }
}

/// 줄인 주문이 사용자가 본 요청보다 크지 않은가: `0 < 수량 ≤ 요청 수량`이고
/// `수량 × 지정가 ≤ 요청 수량 × 요청 값`(같으면 된다). 지정가가 없거나 셈이 넘치면 아니다.
fn within_request(subject: &OrderSubject, order: &ApprovedOrder) -> bool {
    let asked = Decimal::from(subject.quantity);
    if order.quantity <= Decimal::ZERO || order.quantity > asked {
        return false;
    }
    let amount = order
        .price
        .and_then(|price| order.quantity.checked_mul(price));
    let ceiling = asked.checked_mul(subject.price);
    matches!((amount, ceiling), (Some(amount), Some(ceiling)) if amount <= ceiling)
}

#[cfg(test)]
mod tests {
    use pi_approval::SignalKind;
    use pi_market::{Market, Symbol};
    use pi_paper::AccountKind;
    use pi_store::AccountKind as StoreAccount;

    use super::*;

    fn asked(quantity: u64, price: Decimal) -> OrderSubject {
        OrderSubject {
            account: StoreAccount::Paper,
            symbol: "005930".to_string(),
            side: SubjectSide::Buy,
            quantity,
            price,
            order_type: SubjectOrderType::Limit,
            signal_id: "trade-1".to_string(),
            signal_kind: SignalKind::Buy,
            signal_valid_until: UnixMillis(0),
        }
    }

    fn order(quantity: Decimal, price: Option<Decimal>) -> ApprovedOrder {
        ApprovedOrder {
            market: Market::Kr,
            symbol: Symbol::new("005930"),
            side: Side::Buy,
            order_type: OrderType::Limit,
            quantity,
            price,
            account_kind: AccountKind::Paper,
        }
    }

    #[test]
    fn amount_equal_to_the_request_is_within_and_one_over_is_not() {
        let subject = asked(10, Decimal::from(70_000));
        assert!(within_request(
            &subject,
            &order(Decimal::from(7), Some(Decimal::from(100_000)))
        ));
        assert!(!within_request(
            &subject,
            &order(Decimal::from(7), Some(Decimal::from(100_001)))
        ));
    }

    #[test]
    fn order_without_a_price_is_not_within() {
        let subject = asked(10, Decimal::from(70_000));
        assert!(!within_request(&subject, &order(Decimal::from(4), None)));
    }

    #[test]
    fn overflowing_amounts_are_not_within() {
        // 요청 쪽이 넘친다.
        let subject = asked(u64::MAX, Decimal::MAX);
        assert!(!within_request(
            &subject,
            &order(Decimal::ONE, Some(Decimal::ONE))
        ));
        // 주문 쪽이 넘친다.
        let subject = asked(10, Decimal::from(70_000));
        assert!(!within_request(
            &subject,
            &order(Decimal::from(2), Some(Decimal::MAX))
        ));
    }

    // 8차 3: 지정가가 0 이하이면 요청 안이 아니다.
    #[test]
    fn zero_price_is_not_within() {
        let subject = asked(10, Decimal::from(70_000));
        assert!(!within_request(
            &subject,
            &order(Decimal::from(4), Some(Decimal::ZERO))
        ));
    }

    #[test]
    fn negative_price_is_not_within() {
        let subject = asked(10, Decimal::from(70_000));
        assert!(!within_request(
            &subject,
            &order(Decimal::from(4), Some(Decimal::from(-1)))
        ));
    }

    #[test]
    fn negative_quantity_is_not_within() {
        let subject = asked(10, Decimal::from(70_000));
        assert!(!within_request(
            &subject,
            &order(Decimal::from(-1), Some(Decimal::from(70_000)))
        ));
    }
}
