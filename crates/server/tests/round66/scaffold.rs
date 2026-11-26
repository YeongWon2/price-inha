//! 발판이 지금 구현에서 서는가: 서버가 뜨고 · 논의로 사서 보유중이 되고 · 익절로 팔리고 · 하루 마감을 부를 수 있다.
//! 이 파일의 시험은 구현 전에도 통과하는 것이 정상이다.

use pi_rules::Purpose;

use crate::common::{Bench, ENTRY, HYNIX};

// 검증: V99 R8 (발판: 논의 → 보유중 → 익절로 팔림 → 하루 마감)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판이_뜨고_논의로_사서_익절로_팔고_마감을_부를_수_있다() {
    let bench = Bench::start().await;
    let trade = bench.hold_from_discussion(HYNIX).await;
    bench
        .sell(trade, HYNIX, Purpose::TakeProfit, ENTRY + 4_000)
        .await;
    bench.close_day().await;
    bench.close().await;
}
