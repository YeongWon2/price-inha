//! 4번: 시연 조립으로 띄우면 깨우기 엔진의 장 시작 작업(`team-open`) 영수증이 처음부터 `ok`로 끝나고,
//! "꺼졌다 켜져 중단됨"(엔진을 첫 tick 뒤 다시 시작해 남던 것)이 없다. 저장소의 영수증 조회로 본다.

use pi_store::{TriggerReceipt, TriggerReceiptStatus};

use crate::common::{Bench, OPEN_JOB, Plan, kst, saturday, stays_false, wait_for};

/// `team-open` 작업의 영수증들.
fn open_receipts(bench: &Bench) -> Vec<TriggerReceipt> {
    bench
        .store()
        .trigger_receipts()
        .unwrap()
        .into_iter()
        .filter(|receipt| receipt.key.job == OPEN_JOB)
        .collect()
}

// 검증: V54 D14 C33
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_조립으로_띄운_뒤_team_open_영수증은_ok로_끝나고_중단됨이_없다() {
    let bench = Plan::demo().start(kst(saturday(), 22, 30)).await;
    wait_for("team-open 영수증이 열리고 닫힌다", || {
        let receipts = open_receipts(&bench);
        !receipts.is_empty()
            && receipts
                .iter()
                .all(|receipt| receipt.status != TriggerReceiptStatus::Running)
    })
    .await;
    stays_false("team-open 영수증의 중단됨", || {
        open_receipts(&bench)
            .iter()
            .any(|receipt| receipt.status == TriggerReceiptStatus::Interrupted)
    })
    .await;
    let receipts = open_receipts(&bench);
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.status == TriggerReceiptStatus::Ok),
        "team-open 영수증은 처음부터 ok로 끝난다: {receipts:?}"
    );
    bench.close().await;
}
