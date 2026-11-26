//! 발판 시험: 이 폴더의 도우미가 지금 구현에서 서는가.

use crate::common::{Bench, close_of, day, events, monday};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 발판은_시연이_아닌_조립으로_하루를_지내고_그_날_종가를_남긴다() {
    let bench = Bench::start(&[]).await;
    let trade = bench.judged_buy().await;
    let saved = bench.store().get_trade(trade).expect("매매가 있다");
    assert!(saved.decided_at.is_some(), "판단 시각이 있다");

    bench.live_day(monday(), 100_000).await;
    assert_eq!(
        close_of(&bench.store(), monday()).as_deref(),
        Some("100000"),
        "실제 시세 모드에서도 그 날 정규장 마지막 시세가 저장소에 남는다"
    );
    bench.live_day(day(1), 101_000).await;
    assert_eq!(close_of(&bench.store(), day(1)).as_deref(), Some("101000"));
    assert_eq!(events(&bench.store(), "day_close").len(), 2);
    assert_eq!(bench.memos().await.len(), 1);
    bench.close().await;
}
