//! 발판 시험: 이 폴더의 도우미가 지금 구현에서 서는가.

use crate::common::{Bench, MEMO_SYMBOL, days_after, decision_day, events, kst};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판은_판단된_매매를_만들고_종가를_남기고_며칠_뒤_마감을_부를_수_있다() {
    let bench = Bench::start().await;
    let trade = bench.judged_buy(MEMO_SYMBOL).await;
    let saved = bench.store().get_trade(trade).expect("매매가 있다");
    assert!(saved.decided_at.is_some(), "판단 시각이 있다");
    assert_eq!(saved.symbol, MEMO_SYMBOL);

    bench.close_on(decision_day(), MEMO_SYMBOL, 100_000);
    assert_eq!(events(&bench.store(), "reference_prices").len(), 1);

    bench.set(kst(days_after(5), 16, 0));
    assert_eq!(bench.now(), kst(days_after(5), 16, 0));
    bench.close_day().await;

    let reply = bench.get("/api/trades?max=1").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        bench.outside_ai_requests().await,
        0,
        "바깥 AI를 부르지 않는다"
    );
    bench.close().await;
}
