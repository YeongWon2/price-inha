//! 발판이 지금 구현에서 서는가.

use pi_journal::Mistake;

use crate::common::{Bench, MEMO_SYMBOL};

// 검증: V104 (발판: 서버가 뜨고, 팔린 매매에 복기를 넣고, 사기 논의 · 하루 마감을 부를 수 있다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판이_뜨고_복기를_넣고_논의와_마감을_부를_수_있다() {
    let bench = Bench::start().await;
    let trades = bench.reviewed_with(2, &Mistake::LateStop);
    assert_eq!(trades.len(), 2);
    let _ = bench.judged_buy(MEMO_SYMBOL).await;
    bench.close_day().await;
    assert_eq!(
        bench.outside_ai_requests().await,
        0,
        "시연 모드는 바깥 AI를 부르지 않는다"
    );
    bench.close().await;
}
