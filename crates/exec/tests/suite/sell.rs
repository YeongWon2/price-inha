//! 파는 주문: 팔림 전이 · 손절 전 열린 매도 취소 · 실패한 매도의 다시 내기(물러서기).

use pi_exec::{Error, SELL_RETRY_FIRST, SELL_RETRY_MAX, SellPurpose, SellRetry, TradeChange};
use pi_paper::{BrokerError, Fault, OrderGroup, OrderStatus, Side};
use pi_store::TradeState;

use crate::common::{Bench, approved, span};

// 검증: C10
#[tokio::test]
async fn filled_sell_moves_the_trade_to_sold() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "70000"))
        .await
        .unwrap();
    bench.set(2);

    let changes = bench.exec.on_quote(&bench.quote("70000")).unwrap();

    assert!(changes.contains(&TradeChange {
        trade_id: id,
        to: TradeState::Sold,
    }));
    assert_eq!(bench.trade(id).state, TradeState::Sold);
    assert!(bench.exec.guards().is_empty());
    assert!(bench.snapshot_saved());
}

// 검증: C10
#[tokio::test]
async fn stop_loss_cancels_open_sells_of_the_symbol_first() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let take_profit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "80000"))
        .await
        .unwrap();

    let stop = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await
        .unwrap();

    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order_id, stop);
    let closed = bench.orders(OrderGroup::Closed).await;
    let cancelled = closed.iter().find(|o| o.order_id == take_profit).unwrap();
    assert_eq!(cancelled.status, OrderStatus::Canceled);
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: C10
#[tokio::test]
async fn liquidation_cancels_open_sells_of_the_symbol_first() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "80000"))
        .await
        .unwrap();

    let liquidation = bench
        .exec
        .sell(bench.sell(id, SellPurpose::Liquidation, "10", "69000"))
        .await
        .unwrap();

    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order_id, liquidation);
}

// 검증: C10
#[tokio::test]
async fn sell_needs_a_holding_trade() {
    let bench = Bench::new().await;
    let id = bench.judged();

    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;

    assert!(matches!(result, Err(Error::NotHolding(TradeState::Judged))));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E33
#[tokio::test]
async fn buy_permit_on_the_sell_path_is_refused() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let mut submission = bench.sell(id, SellPurpose::StopLoss, "10", "69000");
    submission.permit = bench.permit(id, Side::Buy, "10", "69000");

    let result = bench.exec.sell(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E30
#[tokio::test]
async fn failed_sell_is_an_anomaly_stop_and_waits_to_be_sent_again() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));

    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;

    assert!(matches!(
        result,
        Err(Error::AnomalyStop(BrokerError::Unavailable))
    ));
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(
        bench.exec.pending_sells(),
        vec![SellRetry {
            trade_id: id,
            purpose: SellPurpose::StopLoss,
            order: approved(Side::Sell, "10", "69000"),
            attempts: 1,
            retry_at: bench.now() + span(SELL_RETRY_FIRST),
        }]
    );
}

// 검증: E30
#[tokio::test]
async fn silent_broker_on_a_sell_also_waits_to_be_sent_again() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::NoResponse));

    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::Liquidation, "10", "69000"))
        .await;

    assert!(matches!(
        result,
        Err(Error::AnomalyStop(BrokerError::NoResponse))
    ));
    assert_eq!(bench.exec.pending_sells().len(), 1);
}

// 검증: E30
#[tokio::test]
async fn sell_is_not_sent_again_before_its_time() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;
    let due = bench.now() + span(SELL_RETRY_FIRST);
    bench.exec.broker().set_fault(None);

    let result = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "69000"))
        .await;

    assert!(matches!(result, Err(Error::RetryNotDue { retry_at }) if retry_at == due));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert_eq!(bench.exec.pending_sells()[0].attempts, 1);
}

// 검증: E30
#[tokio::test]
async fn interval_doubles_up_to_the_longest() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;

    for attempt in 2..=10u32 {
        let retry_at = bench.exec.pending_sells()[0].retry_at;
        bench.set_at(retry_at);

        let result = bench
            .exec
            .retry_sell(id, bench.permit(id, Side::Sell, "10", "69000"))
            .await;

        assert!(matches!(
            result,
            Err(Error::AnomalyStop(BrokerError::Unavailable))
        ));
        let pending = bench.exec.pending_sells();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].attempts, attempt);
        let doubled = SELL_RETRY_FIRST * 2u32.pow(attempt - 1);
        let expected = doubled.min(SELL_RETRY_MAX);
        assert_eq!(pending[0].retry_at, bench.now() + span(expected));
    }
}

// 검증: E30
#[tokio::test]
async fn retry_with_different_content_is_refused() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;
    bench.exec.broker().set_fault(None);
    bench.set_at(bench.exec.pending_sells()[0].retry_at);

    let result = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "68000"))
        .await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert_eq!(bench.exec.pending_sells()[0].attempts, 1);
}

// 검증: E30
#[tokio::test]
async fn retry_goes_through_once_the_broker_is_back() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;
    bench.exec.broker().set_fault(None);
    bench.set_at(bench.exec.pending_sells()[0].retry_at);

    let order_id = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "69000"))
        .await
        .unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order_id, order_id);
    assert_eq!(open[0].side, Side::Sell);
}

// 검증: E30
#[tokio::test]
async fn retry_without_a_failed_sell_is_refused() {
    let bench = Bench::new().await;
    let id = bench.holding().await;

    let result = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "69000"))
        .await;

    assert!(matches!(result, Err(Error::NoPendingSell(t)) if t == id));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E33
#[tokio::test]
async fn unbound_permit_is_refused_on_the_sell_path() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let mut submission = bench.sell(id, SellPurpose::StopLoss, "10", "69000");
    submission.permit = bench.unbound_permit(Side::Sell, "10", "69000");

    let result = bench.exec.sell(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
}

// 검증: E33
#[tokio::test]
async fn unbound_permit_is_refused_on_the_sell_retry() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;

    let result = bench
        .exec
        .retry_sell(id, bench.unbound_permit(Side::Sell, "10", "69000"))
        .await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
}
