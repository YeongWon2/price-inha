//! 2차: 다시 낼 파는 주문은 스냅샷에 남아 껐다 켠 뒤에도 이어 내고(E30 · V6),
//! 한 시도 묶음은 같은 멱등성 키를 쓴다.

use pi_exec::{Error, SellPurpose};
use pi_paper::{Fault, OrderGroup, Side};
use pi_store::TradeState;

use crate::common::{Bench, config};

// 검증: E30 V6
#[tokio::test]
async fn pending_sell_survives_a_restart() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::NoResponse));
    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;
    assert!(matches!(result, Err(Error::AnomalyStop(_))));
    let before = bench.exec.pending_sells();
    let key = bench.exec.pending_sell_key(id).unwrap();
    assert_eq!(before.len(), 1);

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert_eq!(bench.exec.pending_sells(), before);
    assert_eq!(bench.exec.pending_sell_key(id), Some(key.clone()));

    bench.set_at(before[0].retry_at);
    bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "69000"))
        .await
        .unwrap();

    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].side, Side::Sell);
    assert_eq!(open[0].client_order_id.as_deref(), Some(key.as_str()));
    assert!(bench.exec.pending_sells().is_empty());
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: E30
#[tokio::test]
async fn retries_of_one_sell_keep_the_same_key() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "71000"))
        .await;
    let key = bench.exec.pending_sell_key(id).unwrap();

    bench.set_at(bench.exec.pending_sells()[0].retry_at);
    let again = bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await;
    assert!(matches!(again, Err(Error::AnomalyStop(_))));
    assert_eq!(bench.exec.pending_sells()[0].attempts, 2);
    assert_eq!(bench.exec.pending_sell_key(id), Some(key.clone()));

    bench.exec.broker().set_fault(None);
    bench.set_at(bench.exec.pending_sells()[0].retry_at);
    bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await
        .unwrap();

    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].client_order_id.as_deref(), Some(key.as_str()));
    assert_eq!(bench.exec.pending_sell_key(id), None);
}

// 검증: E30
#[tokio::test]
async fn a_new_sell_starts_a_new_key() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "71000"))
        .await;
    let first = bench.exec.pending_sell_key(id).unwrap();
    bench.exec.broker().set_fault(None);
    bench.set_at(bench.exec.pending_sells()[0].retry_at);
    bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await
        .unwrap();

    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "69000"))
        .await;

    let second = bench.exec.pending_sell_key(id).unwrap();
    assert_ne!(second, first);
}

// 검증: E30 V6
#[tokio::test]
async fn finished_sell_stays_finished_after_a_restart() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let _ = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "10", "71000"))
        .await;
    bench.exec.broker().set_fault(None);
    bench.set_at(bench.exec.pending_sells()[0].retry_at);
    bench
        .exec
        .retry_sell(id, bench.permit(id, Side::Sell, "10", "71000"))
        .await
        .unwrap();

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert!(bench.exec.pending_sells().is_empty());
    assert_eq!(bench.exec.pending_sell_key(id), None);
    assert!(bench.snapshot_saved());
}
