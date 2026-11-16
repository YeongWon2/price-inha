//! 4차: 점검 통과 표시는 그 매매 번호에 묶여야 하고(다른 매매에 묶인 표시로는 주문이 나가지 않는다),
//! 사는 수량은 저장소의 매매 수량과 같아야 한다.

use pi_exec::{Error, SellPurpose, SellSubmission, Submission};
use pi_paper::{CheckPermit, Fault, OrderGroup, Side, UnfilledTimeout};
use pi_store::{TradeId, TradeState};

use crate::common::{Bench, approved, span};

/// 그 매매 번호에 묶인 삼성전자 지정가 표시.
fn tied(bench: &Bench, trade: TradeId, side: Side, quantity: &str, price: &str) -> CheckPermit {
    bench
        .authority
        .approve_for_trade(trade.0, approved(side, quantity, price))
}

/// 표시를 바꾼 10주 70,000원 사기.
fn buy_with(bench: &Bench, id: TradeId, permit: CheckPermit) -> Submission {
    Submission {
        permit,
        ..bench.buy(id, "10", "70000")
    }
}

/// 아무 주문도 나가지 않았다.
async fn nothing_sent(bench: &Bench) {
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E33
#[tokio::test]
async fn buy_permit_tied_to_this_trade_is_ordered() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let permit = tied(&bench, id, Side::Buy, "10", "70000");

    bench
        .exec
        .submit(buy_with(&bench, id, permit))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: E33
#[tokio::test]
async fn buy_permit_tied_to_another_trade_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let other = bench.judged();
    let permit = tied(&bench, other, Side::Buy, "10", "70000");

    let result = bench.exec.submit(buy_with(&bench, id, permit)).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert_eq!(bench.trade(other).state, TradeState::Judged);
    nothing_sent(&bench).await;
}

// 검증: E33
#[tokio::test]
async fn sell_permit_tied_to_another_trade_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let other = bench.judged();
    let submission = SellSubmission {
        trade_id: id,
        permit: tied(&bench, other, Side::Sell, "10", "69000"),
        purpose: SellPurpose::TakeProfit,
        timeout: UnfilledTimeout::SCALP,
    };

    let result = bench.exec.sell(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    nothing_sent(&bench).await;
}

// 검증: E33 E30
#[tokio::test]
async fn retry_with_a_permit_tied_to_another_trade_is_refused() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let other = bench.judged();
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;
    bench.exec.broker().set_fault(None);
    bench.set_at(bench.now() + span(pi_exec::SELL_RETRY_FIRST));

    let result = bench
        .exec
        .retry_sell(id, tied(&bench, other, Side::Sell, "10", "69000"))
        .await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    assert_eq!(bench.exec.pending_sells().len(), 1);
    assert_eq!(bench.exec.pending_sells()[0].attempts, 1);
    nothing_sent(&bench).await;
}

// 검증: E33
#[tokio::test]
async fn retry_with_a_permit_tied_to_this_trade_is_sent() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;
    bench.exec.broker().set_fault(None);
    bench.set_at(bench.now() + span(pi_exec::SELL_RETRY_FIRST));

    bench
        .exec
        .retry_sell(id, tied(&bench, id, Side::Sell, "10", "69000"))
        .await
        .unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: C10
#[tokio::test]
async fn buy_quantity_other_than_the_trade_quantity_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench.store.set_trade_quantity(id, 5).unwrap();

    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    nothing_sent(&bench).await;
}

// 검증: C10
#[tokio::test]
async fn buy_quantity_equal_to_the_trade_quantity_is_ordered() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench.store.set_trade_quantity(id, 10).unwrap();

    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: C10
#[tokio::test]
async fn approved_quantity_other_than_the_trade_quantity_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench.store.set_trade_quantity(id, 10).unwrap();
    bench
        .store
        .transition_trade(id, TradeState::AwaitingApproval, pi_store::UnixMillis(1))
        .unwrap();
    let grant = bench.grant(id, 5, "70000", true);
    let submission = Submission {
        approval: Some(grant),
        ..bench.buy(id, "5", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    assert_eq!(bench.trade(id).state, TradeState::AwaitingApproval);
    nothing_sent(&bench).await;
}
