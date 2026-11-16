//! 손절 감시(3차): 시세가 손절값에 닿으면 논의 없이 손절 주문 안을 만든다(F4 대안 흐름 1a · E34).

use pi_exec::{Error, SellPurpose};
use pi_market::{Market, Symbol};
use pi_paper::{Fault, OrderType, Side};
use pi_rules::{Funding, Purpose, TradeStage};
use pi_store::TradeState;

use crate::common::{Bench, STOP, d, guard};

// 검증: E34
#[tokio::test]
async fn price_at_or_below_the_stop_hits_the_watch() {
    let bench = Bench::new().await;
    let id = bench.holding().await;

    assert!(bench.exec.stop_hits(&bench.quote("65100")).is_empty());
    assert_eq!(
        bench.exec.stop_hits(&bench.quote("65000")),
        vec![guard(id, "10")]
    );
    assert_eq!(
        bench.exec.stop_hits(&bench.quote("60000")),
        vec![guard(id, "10")]
    );
}

// 검증: E34
#[tokio::test]
async fn quote_of_another_symbol_hits_nothing() {
    let bench = Bench::new().await;
    bench.holding().await;
    let mut quote = bench.quote("1");
    quote.symbol = Symbol::new("000660");

    assert!(bench.exec.stop_hits(&quote).is_empty());
}

// 검증: E29
#[tokio::test]
async fn hit_carries_only_what_is_left_after_a_partial_sale() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "4", "70000"))
        .await
        .unwrap();
    bench.set_at(bench.now() + chrono::Duration::seconds(1));
    bench.exec.on_quote(&bench.quote("70000")).unwrap();

    assert_eq!(
        bench.exec.stop_hits(&bench.quote("64000")),
        vec![guard(id, "6")]
    );
}

// 검증: E30
#[tokio::test]
async fn stop_waiting_for_a_retry_is_not_hit_again() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));
    let failed = bench
        .exec
        .sell(bench.sell(id, SellPurpose::StopLoss, "10", "64000"))
        .await;
    assert!(matches!(failed, Err(Error::AnomalyStop(_))));

    assert!(bench.exec.stop_hits(&bench.quote("64000")).is_empty());
}

// 검증: E34
#[tokio::test]
async fn stop_draft_is_a_market_stop_loss_for_what_is_left() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench.set(30);
    let quote = bench.quote("64000");
    let hit = bench.exec.stop_hits(&quote).remove(0);

    let draft = bench.exec.stop_draft(&hit, &quote).unwrap();

    assert_eq!(draft.market, Market::Kr);
    assert_eq!(draft.symbol, Symbol::new(crate::common::SAM));
    assert_eq!(draft.purpose, Purpose::StopLoss);
    assert_eq!(draft.purpose.side(), Side::Sell);
    assert_eq!(draft.order_type, OrderType::Market);
    assert_eq!(draft.limit_price, None);
    assert_eq!(draft.ai_quantity, Some(d("10")));
    assert_eq!(draft.funding, Funding::Cash);
    assert_eq!(draft.stage, TradeStage::Judged);
    assert_eq!(draft.decided_at, bench.now());
    assert_eq!(
        draft.rationale.stop,
        Some(rust_decimal::Decimal::from(STOP))
    );
    assert_eq!(draft.rationale.entry, Some(d("70000")));
    assert_eq!(draft.rationale.hypothesis.as_deref(), Some("실적 개선"));
    assert!(
        draft
            .sources
            .iter()
            .any(|s| s.name == "시세" && s.known_at == quote.received_at)
    );
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: E34
#[tokio::test]
async fn stop_draft_needs_a_holding_trade() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let quote = bench.quote("64000");

    let result = bench.exec.stop_draft(&guard(id, "10"), &quote);

    assert!(matches!(result, Err(Error::NotHolding(TradeState::Judged))));
}
