//! 발판 확인: 시험 도우미가 지금 구현에서 선다(띄우기 · 매매하기 · 사서 보유중 · 익절로 팔림).

use std::time::Duration;

use pi_rules::Purpose;
use pi_store::TradeState;
use price_inha::Placed;

use crate::common::{Bench, ENTRY, HYNIX, order};

// 검증: V66
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판이_서고_사서_보유중을_거쳐_익절로_팔린다() {
    let bench = Bench::start(Duration::ZERO).await;
    bench.ready().await;
    let trade = bench.hold(HYNIX).await;
    bench.quote(HYNIX, ENTRY + 4_000, 1).await;
    let placed = bench
        .place(order(
            trade,
            HYNIX,
            Purpose::TakeProfit,
            ENTRY + 4_000,
            bench.now(),
        ))
        .await;
    assert!(
        matches!(placed, Placed::Ordered(_)),
        "익절이 나간다: {placed:?}"
    );
    bench.quote(HYNIX, ENTRY + 4_000, 1).await;
    assert_eq!(
        bench.trade_state(trade),
        TradeState::Sold,
        "목표값 시세에 팔린다"
    );
    bench.close().await;
}
