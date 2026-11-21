//! 7차: 팀장 판단 매도(`SellPurpose::TeamExit`). 익절처럼 다른 열린 파는 주문을 취소하지 않고
//! 나가고, 손절 · 청산은 그것까지 먼저 취소하며, 실패해 기다리는 동안에도 손절 감시는 막지 않는다.

use pi_exec::{Error, SELL_RETRY_FIRST, SellPurpose, SellRetry, TradeChange};
use pi_paper::{BrokerError, Fault, OrderGroup, OrderId, OrderStatus, Side};
use pi_store::{TradeId, TradeState};

use crate::common::{Bench, approved, config, guard, span};

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

/// 끝난 주문 가운데 그 번호의 상태.
async fn closed_status(bench: &Bench, id: &OrderId) -> Option<OrderStatus> {
    bench
        .orders(OrderGroup::Closed)
        .await
        .into_iter()
        .find(|order| &order.order_id == id)
        .map(|order| order.status)
}

/// 증권사가 받지 않게 해 두고 그 목적의 매도(10주 · 71,000원)를 내 실패시킨다.
async fn fail_sell(bench: &Bench, id: TradeId, purpose: SellPurpose) -> Result<OrderId, Error> {
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let result = bench
        .exec
        .sell(bench.sell(id, purpose, "10", "71000"))
        .await;
    bench.exec.broker().set_fault(None);
    result
}

// 검증: C10
#[tokio::test]
async fn team_exits_of_two_holdings_of_a_symbol_both_stay_open() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;

    let a = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();
    let b = bench
        .exec
        .sell(bench.sell(second, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();

    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 2);
    assert!(open.iter().all(|order| order.side == Side::Sell));
    let ids = open_ids(&bench).await;
    assert!(ids.contains(&a));
    assert!(ids.contains(&b));
    assert_eq!(closed_status(&bench, &a).await, None);
    assert_eq!(bench.trade(first).state, TradeState::Holding);
    assert_eq!(bench.trade(second).state, TradeState::Holding);
    assert!(bench.exec.pending_sells().is_empty());
}

// 검증: C10
#[tokio::test]
async fn team_exits_of_two_holdings_both_fill_and_both_trades_are_sold() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    for id in [first, second] {
        bench
            .exec
            .sell(bench.sell(id, SellPurpose::TeamExit, "10", "70000"))
            .await
            .unwrap();
    }
    bench.set_at(bench.now() + chrono::Duration::seconds(1));

    let changes = bench.exec.on_quote(&bench.quote("70000")).unwrap();

    for id in [first, second] {
        assert!(changes.contains(&TradeChange {
            trade_id: id,
            to: TradeState::Sold,
        }));
        assert_eq!(bench.trade(id).state, TradeState::Sold);
    }
    assert!(bench.exec.guards().is_empty());
    assert!(bench.holdings().await.is_empty());
    assert!(bench.snapshot_saved());
}

// 검증: C10
#[tokio::test]
async fn team_exit_leaves_an_open_take_profit_of_the_same_trade() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let take_profit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "4", "80000"))
        .await
        .unwrap();

    let team_exit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TeamExit, "6", "71000"))
        .await
        .unwrap();

    let ids = open_ids(&bench).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&take_profit));
    assert!(ids.contains(&team_exit));
}

// 검증: C10
#[tokio::test]
async fn stop_loss_after_team_exits_cancels_every_open_sell_of_the_symbol() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    let a = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();
    let b = bench
        .exec
        .sell(bench.sell(second, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();

    let stop = bench
        .exec
        .sell(bench.sell(first, SellPurpose::StopLoss, "10", "69000"))
        .await
        .unwrap();

    assert_eq!(open_ids(&bench).await, vec![stop]);
    assert_eq!(closed_status(&bench, &a).await, Some(OrderStatus::Canceled));
    assert_eq!(closed_status(&bench, &b).await, Some(OrderStatus::Canceled));
    assert_eq!(bench.trade(first).state, TradeState::Holding);
    assert_eq!(bench.trade(second).state, TradeState::Holding);
}

// 검증: C10
#[tokio::test]
async fn liquidation_after_team_exits_cancels_every_open_sell_of_the_symbol() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    let a = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();
    let b = bench
        .exec
        .sell(bench.sell(second, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();

    let liquidation = bench
        .exec
        .sell(bench.sell(second, SellPurpose::Liquidation, "10", "69000"))
        .await
        .unwrap();

    assert_eq!(open_ids(&bench).await, vec![liquidation]);
    assert_eq!(closed_status(&bench, &a).await, Some(OrderStatus::Canceled));
    assert_eq!(closed_status(&bench, &b).await, Some(OrderStatus::Canceled));
}

// 검증: E30
#[tokio::test]
async fn failed_team_exit_is_an_anomaly_stop_and_waits_to_be_sent_again() {
    let bench = Bench::new().await;
    let id = bench.holding().await;

    let result = fail_sell(&bench, id, SellPurpose::TeamExit).await;

    assert!(matches!(
        result,
        Err(Error::AnomalyStop(BrokerError::Unavailable))
    ));
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(
        bench.exec.pending_sells(),
        vec![SellRetry {
            trade_id: id,
            purpose: SellPurpose::TeamExit,
            order: approved(Side::Sell, "10", "71000"),
            attempts: 1,
            retry_at: bench.now() + span(SELL_RETRY_FIRST),
        }]
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E30
#[tokio::test]
async fn silent_broker_on_a_team_exit_also_waits_to_be_sent_again() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::NoResponse));

    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TeamExit, "10", "71000"))
        .await;

    assert!(matches!(
        result,
        Err(Error::AnomalyStop(BrokerError::NoResponse))
    ));
    let pending = bench.exec.pending_sells();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].purpose, SellPurpose::TeamExit);
}

// 검증: E30 C10
#[tokio::test]
async fn retried_team_exit_goes_out_without_cancelling_other_open_sells() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    let a = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();
    let failed = fail_sell(&bench, second, SellPurpose::TeamExit).await;
    assert!(matches!(failed, Err(Error::AnomalyStop(_))));
    let key = bench.exec.pending_sell_key(second).unwrap();

    let early = bench
        .exec
        .retry_sell(second, bench.permit(second, Side::Sell, "10", "71000"))
        .await;
    assert!(matches!(early, Err(Error::RetryNotDue { .. })));
    bench.set_at(bench.exec.pending_sells()[0].retry_at);
    let b = bench
        .exec
        .retry_sell(second, bench.permit(second, Side::Sell, "10", "71000"))
        .await
        .unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 2);
    assert!(open.iter().any(|order| order.order_id == a));
    let sent = open.iter().find(|order| order.order_id == b).unwrap();
    assert_eq!(sent.client_order_id.as_deref(), Some(key.as_str()));
    assert_eq!(closed_status(&bench, &a).await, None);
}

// 검증: E30 E34
#[tokio::test]
async fn stop_is_still_hit_and_sent_while_a_failed_team_exit_waits() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    let failed = fail_sell(&bench, id, SellPurpose::TeamExit).await;
    assert!(matches!(failed, Err(Error::AnomalyStop(_))));
    assert_eq!(bench.exec.pending_sells().len(), 1);
    let quote = bench.quote("65000");

    let hits = bench.exec.stop_hits(&quote);

    assert_eq!(hits, vec![guard(id, "10")]);
    let draft = bench.exec.stop_draft(&hits[0], &quote).unwrap();
    assert_eq!(draft.purpose, pi_rules::Purpose::StopLoss);
    let stop = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "65000"))
        .await
        .unwrap();
    assert_eq!(open_ids(&bench).await, vec![stop]);
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: E30 E34
#[tokio::test]
async fn failed_team_exit_of_one_holding_blocks_no_stop_of_the_symbol() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    let failed = fail_sell(&bench, second, SellPurpose::TeamExit).await;
    assert!(matches!(failed, Err(Error::AnomalyStop(_))));

    assert_eq!(
        bench.exec.stop_hits(&bench.quote("64000")),
        vec![guard(first, "10"), guard(second, "10")]
    );
}

// 검증: E30 V6
#[tokio::test]
async fn failed_team_exit_is_saved_as_team_exit_and_survives_a_restart() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    let failed = fail_sell(&bench, id, SellPurpose::TeamExit).await;
    assert!(matches!(failed, Err(Error::AnomalyStop(_))));
    let before = bench.exec.pending_sells();
    let key = bench.exec.pending_sell_key(id).unwrap();

    let saved = bench.store.latest_paper_snapshot().unwrap().unwrap().state;
    let waiting = saved["pi_exec"]["pending"].as_array().unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0]["trade_id"], id.0);
    assert_eq!(waiting[0]["purpose"], "team_exit");
    assert!(bench.snapshot_saved());

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert_eq!(bench.exec.pending_sells(), before);
    assert_eq!(bench.exec.pending_sells()[0].purpose, SellPurpose::TeamExit);
    assert_eq!(bench.exec.pending_sell_key(id), Some(key));
    assert_eq!(
        bench.exec.stop_hits(&bench.quote("65000")),
        vec![guard(id, "10")]
    );
}

// 검증: C10
#[tokio::test]
async fn take_profits_of_two_holdings_still_both_stay_open() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;

    let a = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "10", "71000"))
        .await
        .unwrap();
    let b = bench
        .exec
        .sell(bench.sell(second, SellPurpose::TakeProfit, "10", "71000"))
        .await
        .unwrap();

    let ids = open_ids(&bench).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&a));
    assert!(ids.contains(&b));
}

// 검증: C10
#[tokio::test]
async fn stop_loss_and_liquidation_still_cancel_the_other_holdings_open_sell() {
    for purpose in [SellPurpose::StopLoss, SellPurpose::Liquidation] {
        let bench = Bench::new().await;
        let (first, second) = two_holdings(&bench).await;
        let take_profit = bench
            .exec
            .sell(bench.sell(first, SellPurpose::TakeProfit, "10", "71000"))
            .await
            .unwrap();

        let sent = bench
            .exec
            .sell(bench.sell(second, purpose, "10", "69000"))
            .await
            .unwrap();

        assert_eq!(open_ids(&bench).await, vec![sent]);
        assert_eq!(
            closed_status(&bench, &take_profit).await,
            Some(OrderStatus::Canceled)
        );
    }
}

// 검증: E30 E34
#[tokio::test]
async fn only_a_waiting_stop_loss_or_liquidation_keeps_the_watch_quiet() {
    for (purpose, quiet) in [
        (SellPurpose::StopLoss, true),
        (SellPurpose::Liquidation, true),
        (SellPurpose::TakeProfit, false),
    ] {
        let bench = Bench::new().await;
        let id = bench.holding().await;
        let failed = fail_sell(&bench, id, purpose).await;
        assert!(matches!(failed, Err(Error::AnomalyStop(_))));
        assert_eq!(bench.exec.pending_sells()[0].purpose, purpose);

        let hits = bench.exec.stop_hits(&bench.quote("65000"));

        assert_eq!(hits.is_empty(), quiet, "{purpose:?}");
    }
}
