//! 체결 알림을 매매 전이로: 체결 · 일부 체결 · 미체결 취소.

use pi_exec::TradeChange;
use pi_paper::OrderGroup;
use pi_store::{BlockKind, TradeState};

use crate::common::{Bench, config_partial, d, guard};

// 검증: C10 V6
#[tokio::test]
async fn full_fill_moves_the_trade_to_holding() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.set(1);

    let changes = bench.exec.on_quote(&bench.quote("70000")).unwrap();

    assert!(changes.contains(&TradeChange {
        trade_id: id,
        to: TradeState::Holding,
    }));
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(bench.exec.guards(), vec![guard(id, "10")]);
    assert!(bench.snapshot_saved());
}

// 검증: E29
#[tokio::test]
async fn partial_fill_is_holding_only_the_filled_quantity() {
    let bench = Bench::with(config_partial("4")).await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.set(1);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(bench.exec.guards(), vec![guard(id, "4")]);

    bench.set(2);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();

    assert_eq!(bench.exec.guards(), vec![guard(id, "8")]);
}

// 검증: E29 E12
#[tokio::test]
async fn rest_of_a_partial_fill_is_cancelled_and_the_stop_covers_the_filled_part() {
    let bench = Bench::with(config_partial("4")).await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.set(1);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();
    bench.set(61);

    bench.exec.on_time().unwrap();

    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(bench.exec.guards(), vec![guard(id, "4")]);
    let holdings = bench.holdings().await;
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings[0].quantity, d("4"));
    assert!(bench.snapshot_saved());
}

// 검증: E12
#[tokio::test]
async fn unfilled_order_is_cancelled_with_a_reason() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.set(61);

    let changes = bench.exec.on_time().unwrap();

    assert!(changes.contains(&TradeChange {
        trade_id: id,
        to: TradeState::Blocked,
    }));
    let trade = bench.trade(id);
    assert_eq!(trade.state, TradeState::Blocked);
    let reason = trade.blocked.unwrap();
    assert_eq!(reason.kind, BlockKind::UnfilledCancelled);
    assert!(!reason.detail.trim().is_empty());
    assert!(bench.exec.guards().is_empty());
    assert!(bench.snapshot_saved());
}

// 검증: E12
#[tokio::test]
async fn cancellation_noticed_by_another_call_is_not_missed() {
    let bench = Bench::new().await;
    let first = bench.judged();
    bench
        .exec
        .submit(bench.buy(first, "10", "70000"))
        .await
        .unwrap();
    bench.set(61);
    let second = bench.judged();
    // 주문 내기가 먼저 마감 지난 첫 주문을 취소한다. 그 알림은 구독으로만 온다.
    bench
        .exec
        .submit(bench.buy(second, "10", "70000"))
        .await
        .unwrap();

    bench.exec.on_time().unwrap();

    assert_eq!(bench.trade(first).state, TradeState::Blocked);
    assert_eq!(bench.trade(second).state, TradeState::Ordered);
}

// 검증: E12
#[tokio::test]
async fn quote_that_does_not_reach_the_limit_keeps_the_trade_ordered() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.set(1);

    let changes = bench.exec.on_quote(&bench.quote("70100")).unwrap();

    assert!(changes.is_empty());
    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert!(bench.exec.guards().is_empty());
}
