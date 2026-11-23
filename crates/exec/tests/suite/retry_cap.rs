//! 9차: 다시 내기(`retry_sell`)도 매매별 열린 매도 수량 상한을 보고, 기다리는 대기를
//! `forget_pending_sell`로 지운다. 증권사는 가짜(`PaperBroker`)이고 시각은 테스트가 정한다.

use pi_exec::{Error, SellPurpose};
use pi_paper::{Fault, OrderGroup, OrderId, OrderStatus, Side};
use pi_store::{TradeId, TradeState};

use crate::common::{Bench, config};

/// 같은 종목(삼성전자) 보유 둘. 각각 10주다.
async fn two_holdings(bench: &Bench) -> (TradeId, TradeId) {
    let first = bench.holding().await;
    let second = bench.holding().await;
    (first, second)
}

/// 열린 주문의 번호들.
async fn open_ids(bench: &Bench) -> Vec<OrderId> {
    bench
        .orders(OrderGroup::Open)
        .await
        .into_iter()
        .map(|order| order.order_id)
        .collect()
}

/// 증권사가 받지 않게 해 두고 그 목적의 매도를 내 실패시킨다(다시 내기를 기다리게 된다).
async fn fail_sell(bench: &Bench, id: TradeId, purpose: SellPurpose, quantity: &str, price: &str) {
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let result = bench
        .exec
        .sell(bench.sell(id, purpose, quantity, price))
        .await;
    bench.exec.broker().set_fault(None);
    assert!(matches!(result, Err(Error::AnomalyStop(_))));
}

/// 다시 낼 때가 되도록 시각을 옮긴다.
fn make_due(bench: &Bench, id: TradeId) {
    let due = bench
        .exec
        .pending_sells()
        .into_iter()
        .find(|retry| retry.trade_id == id)
        .unwrap()
        .retry_at;
    bench.set_at(due);
}

// 검증: E30 C10
#[tokio::test]
async fn retry_over_the_open_sell_cap_of_the_same_trade_is_too_large_and_keeps_the_wait() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::TeamExit, "10", "71000").await;
    let take_profit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "6", "80000"))
        .await
        .unwrap();
    let before = bench.exec.pending_sells();
    let key = bench.exec.pending_sell_key(id).unwrap();
    make_due(&bench, id);

    let result = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await;

    assert!(matches!(result, Err(Error::SellTooLarge { .. })));
    assert_eq!(open_ids(&bench).await, vec![take_profit]);
    assert_eq!(bench.exec.pending_sells(), before);
    assert_eq!(bench.exec.pending_sells()[0].purpose, SellPurpose::TeamExit);
    assert_eq!(bench.exec.pending_sells()[0].attempts, 1);
    assert_eq!(bench.exec.pending_sell_key(id), Some(key));
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: E30 C10
#[tokio::test]
async fn refused_retry_does_not_call_the_broker_even_when_it_would_fail() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::TeamExit, "10", "71000").await;
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "6", "80000"))
        .await
        .unwrap();
    let before = bench.exec.pending_sells();
    make_due(&bench, id);
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));

    let result = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await;
    bench.exec.broker().set_fault(None);

    // 상한으로 거부하므로 증권사 실패(AnomalyStop)로 횟수가 늘지 않는다.
    assert!(matches!(result, Err(Error::SellTooLarge { .. })));
    assert_eq!(bench.exec.pending_sells(), before);
}

// 검증: E30 C10
#[tokio::test]
async fn retry_within_the_open_sell_cap_goes_out_as_before() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::TeamExit, "4", "71000").await;
    let key = bench.exec.pending_sell_key(id).unwrap();
    let take_profit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "6", "80000"))
        .await
        .unwrap();
    make_due(&bench, id);

    let sent = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "4", "71000"))
        .await
        .unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 2);
    assert!(open.iter().any(|order| order.order_id == take_profit));
    let order = open.iter().find(|order| order.order_id == sent).unwrap();
    assert_eq!(order.client_order_id.as_deref(), Some(key.as_str()));
}

// 검증: E30 C10
#[tokio::test]
async fn open_sell_of_another_trade_does_not_count_in_the_cap_of_a_retry() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    fail_sell(&bench, second, SellPurpose::TeamExit, "10", "71000").await;
    let other = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "10", "80000"))
        .await
        .unwrap();
    make_due(&bench, second);

    let sent = bench
        .exec
        .retry_sell(second, bench.permit(second, Side::Sell, "10", "71000"))
        .await
        .unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    let ids = open_ids(&bench).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&other));
    assert!(ids.contains(&sent));
}

// 검증: E30 E34 C10
#[tokio::test]
async fn stop_loss_retry_cancels_open_sells_of_the_symbol_then_sends_the_whole_quantity() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    fail_sell(&bench, second, SellPurpose::StopLoss, "10", "69000").await;
    let other = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();
    make_due(&bench, second);

    let stop = bench
        .exec
        .retry_sell(second, bench.permit(second, Side::Sell, "10", "69000"))
        .await
        .unwrap();

    assert_eq!(open_ids(&bench).await, vec![stop]);
    let closed = bench.orders(OrderGroup::Closed).await;
    let cancelled = closed.iter().find(|order| order.order_id == other).unwrap();
    assert_eq!(cancelled.status, OrderStatus::Canceled);
    assert!(bench.exec.pending_sells().is_empty());
}

// 검증: E30
#[tokio::test]
async fn forgetting_a_pending_sell_returns_it_and_clears_the_wait_and_its_key() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::TeamExit, "10", "71000").await;
    let waiting = bench.exec.pending_sells();
    assert_eq!(waiting.len(), 1);

    let forgotten = bench.exec.forget_pending_sell(id);

    assert_eq!(forgotten, Some(waiting[0].clone()));
    assert!(bench.exec.pending_sells().is_empty());
    assert_eq!(bench.exec.pending_sell_key(id), None);
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E30
#[tokio::test]
async fn retry_after_forgetting_says_there_is_no_pending_sell() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::TeamExit, "10", "71000").await;
    make_due(&bench, id);
    bench.exec.forget_pending_sell(id).unwrap();

    let result = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await;

    assert!(matches!(result, Err(Error::NoPendingSell(trade)) if trade == id));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E30
#[tokio::test]
async fn forgetting_another_trade_leaves_a_pending_sell_alone() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    fail_sell(&bench, first, SellPurpose::TeamExit, "10", "71000").await;
    fail_sell(&bench, second, SellPurpose::TeamExit, "10", "71000").await;
    let second_wait = bench
        .exec
        .pending_sells()
        .into_iter()
        .find(|retry| retry.trade_id == second)
        .unwrap();
    let second_key = bench.exec.pending_sell_key(second).unwrap();

    let forgotten = bench.exec.forget_pending_sell(first).unwrap();

    assert_eq!(forgotten.trade_id, first);
    assert_eq!(bench.exec.pending_sells(), vec![second_wait]);
    assert_eq!(bench.exec.pending_sell_key(first), None);
    assert_eq!(bench.exec.pending_sell_key(second), Some(second_key));
}

// 검증: E30
#[tokio::test]
async fn forgetting_a_trade_without_a_wait_gives_none_and_forgetting_twice_too() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    assert_eq!(bench.exec.forget_pending_sell(id), None);
    assert_eq!(bench.exec.forget_pending_sell(TradeId(9_999)), None);

    fail_sell(&bench, id, SellPurpose::TeamExit, "10", "71000").await;
    assert!(bench.exec.forget_pending_sell(id).is_some());

    assert_eq!(bench.exec.forget_pending_sell(id), None);
}

// 검증: E30 E34
#[tokio::test]
async fn a_stop_loss_wait_can_be_forgotten_and_then_a_take_profit_is_no_longer_refused() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::StopLoss, "10", "69000").await;
    let refused = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "80000"))
        .await;
    assert!(matches!(refused, Err(Error::StopPending)));

    let forgotten = bench.exec.forget_pending_sell(id).unwrap();

    assert_eq!(forgotten.purpose, SellPurpose::StopLoss);
    let sent = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "80000"))
        .await
        .unwrap();
    assert_eq!(open_ids(&bench).await, vec![sent]);
}

// 검증: E30 V6
#[tokio::test]
async fn forgotten_wait_is_saved_and_does_not_come_back_after_a_restart() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    fail_sell(&bench, id, SellPurpose::TeamExit, "10", "71000").await;
    bench.exec.forget_pending_sell(id).unwrap();

    let saved = bench.store.latest_paper_snapshot().unwrap().unwrap().state;
    assert!(saved["pi_exec"]["pending"].as_array().unwrap().is_empty());

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    assert_eq!(bench.exec.pending_sell_key(id), None);
}
