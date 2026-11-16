//! 손절 감시(F4 1a): 보유한 매매의 시세가 손절값에 닿으면 논의 없이 손절 주문을 낸다.

use pi_rules::Purpose;
use pi_store::TradeState;
use price_inha::Placed;

use crate::common::{Bench, ENTRY, HYNIX, STOP, kst, monday, order};

/// 사서 보유중까지 만든 매매.
async fn holding(bench: &Bench) -> pi_store::TradeId {
    let at = bench.now();
    bench.quote(HYNIX, ENTRY, at).await;
    let trade = bench.judged(HYNIX);
    let placed = bench
        .place(order(trade, HYNIX, Purpose::NewEntry, ENTRY, at))
        .await;
    assert!(matches!(placed, Placed::Ordered(_)), "산다: {placed:?}");
    bench
        .quote(HYNIX, ENTRY, at + chrono::Duration::seconds(1))
        .await;
    assert_eq!(bench.trade_state(trade), TradeState::Holding);
    trade
}

// 검증: F4 E34 C10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시세가_손절값_아래로_가면_논의_없이_손절로_판다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let trade = holding(&bench).await;
    let at = bench.now() + chrono::Duration::seconds(1);
    bench.quote(HYNIX, STOP - 1_000, at).await;
    bench
        .quote(HYNIX, STOP - 1_000, at + chrono::Duration::seconds(1))
        .await;
    assert_eq!(
        bench.trade_state(trade),
        TradeState::Sold,
        "손절값에 닿으면 손절 주문이 나가 팔린다"
    );
    bench.close().await;
}

// 검증: F4 E30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 손절이_나간_매매에_손절을_또_내도_두_번_팔지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let trade = holding(&bench).await;
    let at = bench.now() + chrono::Duration::seconds(1);
    bench.quote(HYNIX, STOP - 1_000, at).await;
    let first = bench
        .place(order(trade, HYNIX, Purpose::StopLoss, STOP - 1_000, at))
        .await;
    let again = bench
        .place(order(trade, HYNIX, Purpose::StopLoss, STOP - 1_000, at))
        .await;
    assert!(matches!(first, Placed::Ordered(_)), "{first:?}");
    assert_eq!(first, again, "이미 나간 손절 주문을 돌려준다");
    bench
        .quote(HYNIX, STOP - 1_000, at + chrono::Duration::seconds(1))
        .await;
    assert_eq!(bench.trade_state(trade), TradeState::Sold);
    bench.close().await;
}
