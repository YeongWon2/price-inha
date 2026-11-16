//! 보유 장부를 스냅샷에 남긴다(3차): 모의계좌가 끝난 주문을 덜어 낸 뒤 다시 켜도 손절 감시와
//! 파는 수량 검사가 남는다.

use pi_exec::{Error, SellPurpose};
use pi_store::UnixMillis;

use crate::common::{Bench, config, d, guard};

/// 끝난 주문을 모의계좌에서 덜어 내고 그 계좌를 저장하게 한다.
fn archive_all(bench: &Bench) {
    let later = bench.now() + chrono::Duration::seconds(1);
    let archived = bench.exec.broker().archive_before(later);
    assert!(!archived.orders.is_empty());
    bench.exec.on_time().unwrap();
}

/// 파는 수량이 남은 수량을 넘으면 막히는가.
async fn sell_over(bench: &Bench, id: pi_store::TradeId, quantity: &str, left: &str) {
    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, quantity, "70000"))
        .await;
    assert!(
        matches!(result, Err(Error::SellTooLarge { remaining }) if remaining == d(left)),
        "{result:?}"
    );
}

// 검증: V6
#[tokio::test]
async fn stop_watch_survives_a_restart_after_orders_are_archived() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    archive_all(&bench);

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert_eq!(bench.exec.guards(), vec![guard(id, "10")]);
    sell_over(&bench, id, "11", "10").await;
}

// 검증: V6
#[tokio::test]
async fn partial_sale_survives_a_restart_after_orders_are_archived() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "4", "70000"))
        .await
        .unwrap();
    bench.exec.on_quote(&bench.quote("70000")).unwrap();
    archive_all(&bench);

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert_eq!(bench.exec.guards(), vec![guard(id, "6")]);
    sell_over(&bench, id, "7", "6").await;
}

// 검증: V6
#[tokio::test]
async fn older_snapshot_without_a_ledger_rebuilds_it_from_the_holdings() {
    let mut bench = Bench::new().await;
    let id = bench.holding().await;
    archive_all(&bench);
    let mut doc = bench.store.latest_paper_snapshot().unwrap().unwrap().state;
    doc["pi_exec"].as_object_mut().unwrap().remove("positions");
    bench
        .store
        .save_paper_snapshot(&doc, UnixMillis(1))
        .unwrap();

    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();

    assert_eq!(bench.exec.guards(), vec![guard(id, "10")]);
    sell_over(&bench, id, "11", "10").await;
}
