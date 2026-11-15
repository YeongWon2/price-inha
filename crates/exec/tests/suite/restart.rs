//! 껐다 켜기: 스냅샷으로 계좌 되살리기(V6), 증권사와 맞춰 보기 전에는 새 주문 거부(E16).

use pi_exec::{Error, Reconciled, SellPurpose};
use pi_paper::{OrderGroup, Side};
use pi_store::{BlockKind, TradeState, UnixMillis};

use crate::common::{Bench, config, guard};

// 검증: E16
#[tokio::test]
async fn new_executor_takes_no_order_until_reconciled() {
    let bench = Bench::unready(config());
    let id = bench.judged();

    assert!(!bench.exec.ready());
    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;
    assert!(matches!(result, Err(Error::NotReady)));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());

    bench.exec.reconcile().await.unwrap();

    assert!(bench.exec.ready());
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    assert_eq!(bench.trade(id).state, TradeState::Ordered);
}

// 검증: V6
#[tokio::test]
async fn restart_brings_back_the_account() {
    let mut bench = Bench::new().await;
    bench.holding().await;
    let before = bench.exec.broker().snapshot();

    bench.reopen(config());

    assert_eq!(bench.exec.broker().snapshot(), before);
    let holdings = bench.holdings().await;
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings[0].quantity, crate::common::d("10"));
}

// 검증: V6
#[tokio::test]
async fn bad_snapshot_is_not_replaced_by_a_fresh_account() {
    let bench = Bench::new().await;
    bench
        .store
        .save_paper_snapshot(&serde_json::json!({ "version": 99 }), UnixMillis(1))
        .unwrap();

    let result = bench.try_open(config());

    assert!(matches!(result, Err(Error::Snapshot(_))));
}

// 검증: E16
#[tokio::test]
async fn restarted_executor_takes_no_order_until_reconciled() {
    let mut bench = Bench::new().await;
    let held = bench.holding().await;
    bench.reopen(config());
    let id = bench.judged();

    assert!(!bench.exec.ready());
    let buy = bench.exec.submit(bench.buy(id, "10", "70000")).await;
    let sell = bench
        .exec
        .sell(bench.sell(held, SellPurpose::StopLoss, "10", "69000"))
        .await;

    assert!(matches!(buy, Err(Error::NotReady)));
    assert!(matches!(sell, Err(Error::NotReady)));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E16 V6
#[tokio::test]
async fn reconcile_moves_a_fill_missed_while_down_to_holding() {
    let mut bench = Bench::new().await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.reopen(config());
    bench.set(1);
    // 실행기가 처리하지 못한 체결(맞춰 보기 전에 계좌에서 일어남).
    bench
        .exec
        .broker()
        .on_quote(&bench.quote("70000"), bench.now());

    let reconciled = bench.exec.reconcile().await.unwrap();

    assert_eq!(
        reconciled,
        Reconciled {
            holding: vec![id],
            blocked: vec![],
            open: vec![],
        }
    );
    assert!(bench.exec.ready());
    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(bench.exec.guards(), vec![guard(id, "10")]);
}

// 검증: E16
#[tokio::test]
async fn reconcile_blocks_an_ordered_trade_the_broker_never_got() {
    let mut bench = Bench::new().await;
    let id = bench.judged();
    // 주문됨으로 저장한 뒤 주문을 내기 전에 꺼진 것.
    bench
        .store
        .transition_trade(id, TradeState::Ordered, UnixMillis(1))
        .unwrap();
    bench.reopen(config());

    let reconciled = bench.exec.reconcile().await.unwrap();

    assert_eq!(reconciled.blocked, vec![id]);
    let trade = bench.trade(id);
    assert_eq!(trade.state, TradeState::Blocked);
    let reason = trade.blocked.unwrap();
    assert_eq!(reason.kind, BlockKind::UnfilledCancelled);
    assert!(!reason.detail.trim().is_empty());
}

// 검증: E16 E12
#[tokio::test]
async fn reconcile_blocks_an_order_that_ended_unfilled_while_down() {
    let mut bench = Bench::new().await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.reopen(config());
    bench.set(61);

    let reconciled = bench.exec.reconcile().await.unwrap();

    assert_eq!(reconciled.blocked, vec![id]);
    assert_eq!(bench.trade(id).state, TradeState::Blocked);
}

// 검증: E16
#[tokio::test]
async fn reconcile_keeps_an_open_order_and_follows_it() {
    let mut bench = Bench::new().await;
    let id = bench.judged();
    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();
    bench.reopen(config());

    let reconciled = bench.exec.reconcile().await.unwrap();

    assert_eq!(reconciled.open, vec![id]);
    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    bench.set(1);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: E16 V6
#[tokio::test]
async fn reconcile_restores_the_stop_watch_of_holding_trades() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    bench.reopen(config());

    bench.exec.reconcile().await.unwrap();

    assert_eq!(bench.exec.guards(), vec![guard(id, "10")]);
}

// 검증: E16
#[tokio::test]
async fn permit_from_before_the_restart_is_not_accepted() {
    let mut bench = Bench::new().await;
    let id = bench.judged();
    let old = bench.permit(Side::Buy, "10", "70000");
    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();
    let submission = pi_exec::Submission {
        permit: old,
        ..bench.buy(id, "10", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(
        result,
        Err(Error::Rejected(pi_paper::BrokerError::CheckInvalid))
    ));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}
