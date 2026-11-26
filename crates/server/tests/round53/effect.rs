//! 익절 · 손절 연출 이벤트(C26 · E60 · V66 · R5-14): 매매가 팔림이 될 때 판 까닭으로 `office.effect`를 낸다.
//! 수익률은 서버가 세서 글자로 싣는다(화면은 그린다).

use std::time::Duration;

use pi_rules::Purpose;
use pi_store::{TradeId, TradeState};
use price_inha::Placed;
use serde_json::Value;

use crate::common::{Bench, ENTRY, HYNIX, STOP, events, order, stays_false, wait_for};

const KIND: &str = "office.effect";

/// 그 매매의 연출 이벤트들.
fn effects_of(bench: &Bench, trade: TradeId) -> Vec<Value> {
    events(&bench.store(), KIND)
        .into_iter()
        .map(|(_, payload)| payload)
        .filter(|p| p["trade"].as_i64() == Some(trade.0))
        .collect()
}

/// 수익률 글자를 수로.
fn pct(effect: &Value) -> f64 {
    effect["return_pct"]
        .as_str()
        .unwrap_or_else(|| panic!("수익률은 글자로 싣는다: {effect}"))
        .parse()
        .unwrap_or_else(|_| panic!("수익률 글자는 수다: {effect}"))
}

// 검증: V66 C26 R5-14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 익절로_팔리면_매매_담당에게_익절_연출과_수익률이_실린다() {
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
    assert!(matches!(placed, Placed::Ordered(_)), "{placed:?}");
    assert!(
        effects_of(&bench, trade).is_empty(),
        "팔리기 전에는 내지 않는다"
    );
    bench.quote(HYNIX, ENTRY + 4_000, 1).await;
    assert_eq!(bench.trade_state(trade), TradeState::Sold);
    wait_for("익절 연출 이벤트", || {
        !effects_of(&bench, trade).is_empty()
    })
    .await;
    let effect = effects_of(&bench, trade).remove(0);
    assert_eq!(effect["kind"], "take_profit", "{effect}");
    assert_eq!(effect["agent"], "trader", "연출은 매매 담당에게: {effect}");
    assert_eq!(effect["symbol"], HYNIX, "{effect}");
    assert_eq!(
        effect["at"].as_i64(),
        Some(bench.now().timestamp_millis()),
        "{effect}"
    );
    assert_eq!(
        effect["return_pct"], "4.00",
        "산 값 100,000 · 판 값 104,000: {effect}"
    );
    // 시세가 더 와도 한 매매에 한 번만 낸다.
    bench.quote(HYNIX, ENTRY + 4_000, 1).await;
    bench.quote(HYNIX, ENTRY + 5_000, 1).await;
    assert_eq!(effects_of(&bench, trade).len(), 1, "한 번만 낸다");
    bench.close().await;
}

// 검증: V66 C26 R5-14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 손절로_팔리면_손절_연출과_음수_수익률이_실린다() {
    let bench = Bench::start(Duration::ZERO).await;
    bench.ready().await;
    let trade = bench.hold(HYNIX).await;
    // 손절값 아래 시세: 서버가 논의 없이 손절 주문을 내고, 다음 시세에 팔린다.
    bench.quote(HYNIX, STOP - 1_000, 1).await;
    bench.quote(HYNIX, STOP - 1_000, 1).await;
    assert_eq!(bench.trade_state(trade), TradeState::Sold);
    wait_for("손절 연출 이벤트", || {
        !effects_of(&bench, trade).is_empty()
    })
    .await;
    let effect = effects_of(&bench, trade).remove(0);
    assert_eq!(effect["kind"], "stop_loss", "{effect}");
    assert_eq!(effect["agent"], "trader", "{effect}");
    let value = pct(&effect);
    assert!(
        value < -5.0 && value > -7.0,
        "손절값 아래에서 팔렸다: {effect}"
    );
    assert!(
        effect["return_pct"]
            .as_str()
            .is_some_and(|t| t.starts_with('-')),
        "손실은 - 글자로 시작한다: {effect}"
    );
    bench.close().await;
}

// 검증: V66 E60
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팔리지_않은_익절이나_청산은_연출하지_않는다() {
    let bench = Bench::start(Duration::ZERO).await;
    bench.ready().await;
    let waiting = bench.hold(HYNIX).await;
    // 목표값에 닿지 않는 시세: 익절 지정가 주문은 걸려만 있다.
    bench.quote(HYNIX, ENTRY + 1_000, 1).await;
    let placed = bench
        .place(order(
            waiting,
            HYNIX,
            Purpose::TakeProfit,
            ENTRY + 4_000,
            bench.now(),
        ))
        .await;
    assert!(matches!(placed, Placed::Ordered(_)), "{placed:?}");
    bench.quote(HYNIX, ENTRY + 1_000, 1).await;
    assert_eq!(bench.trade_state(waiting), TradeState::Holding);
    stays_false("팔리지 않은 익절은 연출하지 않는다", || {
        !effects_of(&bench, waiting).is_empty()
    })
    .await;
    assert!(
        events(&bench.store(), KIND).is_empty(),
        "청산 · 다른 까닭 없이 연출 이벤트가 없다"
    );
    bench.close().await;
}
