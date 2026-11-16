//! 주문은 나갔는데 스냅샷 저장만 실패하는 경우(2차 결정, 3차 시험): 오류로 끝내지 않고 다음
//! 저장에서 맞춘다.

use pi_exec::SellPurpose;
use pi_paper::OrderGroup;

use crate::common::Bench;

// 검증: V6
#[tokio::test]
async fn sell_goes_out_while_the_snapshot_cannot_be_saved_and_is_saved_later() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    assert!(bench.snapshot_saved());
    let other = rusqlite::Connection::open(bench.db_path()).unwrap();
    other.execute_batch("BEGIN EXCLUSIVE").unwrap();

    let placed = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "4", "71000"))
        .await;

    assert!(placed.is_ok(), "{placed:?}");
    assert!(!bench.snapshot_saved());
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);

    other.execute_batch("ROLLBACK").unwrap();
    bench.exec.on_time().unwrap();

    assert!(bench.snapshot_saved());
}
