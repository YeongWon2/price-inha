//! 8차: 파는 주문 지키기. 손절 · 청산이 다시 내기를 기다리는 매매에는 익절 · 팀장 팔기를 받지 않고
//! (`StopPending`), 한 매매의 파는 수량은 그 매매에 이미 열린 파는 주문의 남은 수량까지 빼고 본다.

use pi_exec::{Error, SellPurpose};
use pi_paper::{Fault, OrderGroup, OrderId, OrderStatus, Side};
use pi_store::{TradeId, TradeState};

use crate::common::{Bench, config_partial, d};

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
async fn fail_sell(bench: &Bench, id: TradeId, purpose: SellPurpose) {
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let result = bench
        .exec
        .sell(bench.sell(id, purpose, "10", "71000"))
        .await;
    bench.exec.broker().set_fault(None);
    assert!(matches!(result, Err(Error::AnomalyStop(_))), "{result:?}");
}

/// 그 매도가 `SellTooLarge`로 거부된다.
async fn too_large(bench: &Bench, id: TradeId, purpose: SellPurpose, quantity: &str) {
    let result = bench
        .exec
        .sell(bench.sell(id, purpose, quantity, "71000"))
        .await;
    assert!(
        matches!(result, Err(Error::SellTooLarge { .. })),
        "{purpose:?} {quantity}: {result:?}"
    );
}

// ── 1. 손절 · 청산의 다시 내기를 지킨다 ─────────────────────────────────────

// 검증: E30 E34 C10
#[tokio::test]
async fn take_profit_and_team_exit_are_refused_while_a_stop_waits() {
    for waiting in [SellPurpose::StopLoss, SellPurpose::Liquidation] {
        for purpose in [SellPurpose::TakeProfit, SellPurpose::TeamExit] {
            let bench = Bench::new().await;
            let id = bench.holding().await;
            fail_sell(&bench, id, waiting).await;
            let before = bench.exec.pending_sells();
            let key = bench.exec.pending_sell_key(id);
            let closed = bench.orders(OrderGroup::Closed).await.len();

            let result = bench
                .exec
                .sell(bench.sell(id, purpose, "10", "72000"))
                .await;

            assert!(
                matches!(result, Err(Error::StopPending)),
                "{waiting:?} → {purpose:?}: {result:?}"
            );
            let after = bench.exec.pending_sells();
            assert_eq!(after, before, "{waiting:?} → {purpose:?}");
            assert_eq!(after[0].purpose, waiting);
            assert_eq!(after[0].attempts, 1);
            assert_eq!(bench.exec.pending_sell_key(id), key);
            assert!(bench.orders(OrderGroup::Open).await.is_empty());
            assert_eq!(bench.orders(OrderGroup::Closed).await.len(), closed);
            assert_eq!(bench.trade(id).state, TradeState::Holding);
        }
    }
}

// 검증: E30 E34
#[tokio::test]
async fn waiting_stop_is_still_retried_after_a_refused_take_profit() {
    for purpose in [SellPurpose::TakeProfit, SellPurpose::TeamExit] {
        let bench = Bench::new().await;
        let id = bench.holding().await;
        fail_sell(&bench, id, SellPurpose::StopLoss).await;
        let key = bench.exec.pending_sell_key(id).unwrap();
        let refused = bench
            .exec
            .sell(bench.sell(id, purpose, "10", "72000"))
            .await;
        assert!(matches!(refused, Err(Error::StopPending)), "{refused:?}");
        bench.set_at(bench.exec.pending_sells()[0].retry_at);

        let sent = bench
            .exec
            .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
            .await
            .unwrap();

        assert!(bench.exec.pending_sells().is_empty());
        let open = bench.orders(OrderGroup::Open).await;
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].order_id, sent);
        assert_eq!(open[0].side, Side::Sell);
        assert_eq!(open[0].quantity, d("10"));
        assert_eq!(open[0].price, Some(d("71000")));
        assert_eq!(open[0].client_order_id.as_deref(), Some(key.as_str()));
    }
}

// 검증: E30 E34
#[tokio::test]
async fn a_new_stop_is_still_taken_while_a_stop_waits() {
    for (waiting, purpose) in [
        (SellPurpose::StopLoss, SellPurpose::StopLoss),
        (SellPurpose::StopLoss, SellPurpose::Liquidation),
        (SellPurpose::Liquidation, SellPurpose::StopLoss),
    ] {
        let bench = Bench::new().await;
        let id = bench.holding().await;
        fail_sell(&bench, id, waiting).await;

        let sent = bench
            .exec
            .sell(bench.sell(id, purpose, "10", "69000"))
            .await
            .unwrap();

        assert_eq!(
            open_ids(&bench).await,
            vec![sent],
            "{waiting:?} → {purpose:?}"
        );
        assert!(bench.exec.pending_sells().is_empty());
    }
}

// 검증: E30 E34
#[tokio::test]
async fn stop_still_wins_over_a_waiting_take_profit_or_team_exit() {
    for (waiting, purpose) in [
        (SellPurpose::TakeProfit, SellPurpose::StopLoss),
        (SellPurpose::TeamExit, SellPurpose::Liquidation),
    ] {
        let bench = Bench::new().await;
        let id = bench.holding().await;
        fail_sell(&bench, id, waiting).await;

        let sent = bench
            .exec
            .sell(bench.sell(id, purpose, "10", "69000"))
            .await
            .unwrap();

        assert_eq!(
            open_ids(&bench).await,
            vec![sent],
            "{waiting:?} → {purpose:?}"
        );
        assert!(bench.exec.pending_sells().is_empty());
    }
}

// ── 2. 매매별 열린 매도 수량 상한 ───────────────────────────────────────────

// 검증: C10
#[tokio::test]
async fn open_take_profit_counts_against_a_team_exit_of_the_same_trade() {
    let bench = Bench::new().await;
    let (first, _second) = two_holdings(&bench).await;
    let take_profit = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "10", "80000"))
        .await
        .unwrap();

    too_large(&bench, first, SellPurpose::TeamExit, "10").await;
    too_large(&bench, first, SellPurpose::TakeProfit, "1").await;

    assert_eq!(open_ids(&bench).await, vec![take_profit]);
    assert!(bench.exec.pending_sells().is_empty());
}

// 검증: C10
#[tokio::test]
async fn sells_of_one_trade_up_to_its_holding_all_go_out() {
    let bench = Bench::new().await;
    let (first, _second) = two_holdings(&bench).await;
    let take_profit = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "4", "80000"))
        .await
        .unwrap();
    let team_exit = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TeamExit, "6", "71000"))
        .await
        .unwrap();

    too_large(&bench, first, SellPurpose::TeamExit, "1").await;

    let ids = open_ids(&bench).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&take_profit));
    assert!(ids.contains(&team_exit));
}

// 검증: C10
#[tokio::test]
async fn open_sells_of_another_trade_do_not_count() {
    let bench = Bench::new().await;
    let (first, second) = two_holdings(&bench).await;
    let a = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "10", "80000"))
        .await
        .unwrap();

    let b = bench
        .exec
        .sell(bench.sell(second, SellPurpose::TeamExit, "10", "71000"))
        .await
        .unwrap();

    let ids = open_ids(&bench).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&a));
    assert!(ids.contains(&b));
}

// 검증: C10 E34
#[tokio::test]
async fn stop_cancels_open_sells_first_and_then_sells_everything() {
    for purpose in [SellPurpose::StopLoss, SellPurpose::Liquidation] {
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

        let stop = bench
            .exec
            .sell(bench.sell(id, purpose, "10", "69000"))
            .await
            .unwrap();

        let open = bench.orders(OrderGroup::Open).await;
        assert_eq!(open.len(), 1, "{purpose:?}");
        assert_eq!(open[0].order_id, stop);
        assert_eq!(open[0].quantity, d("10"));
        assert_eq!(
            closed_status(&bench, &take_profit).await,
            Some(OrderStatus::Canceled)
        );
        assert_eq!(
            closed_status(&bench, &team_exit).await,
            Some(OrderStatus::Canceled)
        );
    }
}

// 검증: C10 E29
#[tokio::test]
async fn partly_filled_open_sell_counts_only_what_is_left_of_it() {
    let bench = Bench::with(config_partial("4")).await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    for second in 1..=3 {
        bench.set(second);
        bench.exec.on_quote(&bench.quote("70000")).unwrap();
    }
    assert_eq!(bench.holdings().await[0].quantity, d("10"));
    // 익절 5주 가운데 4주가 체결되고 1주가 열려 있다: 남은 보유 6 · 열린 매도 1.
    let take_profit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "5", "70000"))
        .await
        .unwrap();
    bench.set(4);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();
    assert_eq!(bench.holdings().await[0].quantity, d("6"));
    assert_eq!(open_ids(&bench).await, vec![take_profit.clone()]);

    too_large(&bench, id, SellPurpose::TeamExit, "6").await;
    let team_exit = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TeamExit, "5", "71000"))
        .await
        .unwrap();

    let ids = open_ids(&bench).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&take_profit));
    assert!(ids.contains(&team_exit));
}
