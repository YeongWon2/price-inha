//! 2차: 승인 증표와 점검 통과 표시가 그 매매에 묶인다(다른 매매 · 다른 내용으로는 주문이 나가지 않는다).

use pi_exec::{Error, SellPurpose, Submission};
use pi_paper::OrderGroup;
use pi_rules::ApprovalGrant;
use pi_store::{TradeId, TradeState, UnixMillis};

use crate::common::{Bench, d};

/// 승인대기인 매매 하나.
fn awaiting(bench: &Bench) -> TradeId {
    let id = bench.judged();
    bench
        .store
        .transition_trade(id, TradeState::AwaitingApproval, UnixMillis(1))
        .unwrap();
    id
}

/// 증표를 붙인 10주 70,000원 사기.
fn with_grant(bench: &Bench, id: TradeId, grant: ApprovalGrant) -> Submission {
    Submission {
        approval: Some(grant),
        ..bench.buy(id, "10", "70000")
    }
}

/// 증표가 맞지 않아 아무 주문도 나가지 않았고 매매는 그대로다.
async fn refused(bench: &Bench, id: TradeId, grant: ApprovalGrant, state: TradeState) {
    let result = bench.exec.submit(with_grant(bench, id, grant)).await;

    assert!(matches!(result, Err(Error::ApprovalMismatch)), "{result:?}");
    assert_eq!(bench.trade(id).state, state);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert!(bench.orders(OrderGroup::Closed).await.is_empty());
}

// 검증: C10
#[tokio::test]
async fn grant_of_this_trade_places_the_order() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let grant = bench.grant(id, 10, "70000", true);

    bench
        .exec
        .submit(with_grant(&bench, id, grant))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: C10
#[tokio::test]
async fn grant_of_another_trade_places_nothing() {
    let bench = Bench::new().await;
    let first = awaiting(&bench);
    let second = awaiting(&bench);
    let grant = bench.grant(first, 10, "70000", true);

    refused(&bench, second, grant, TradeState::AwaitingApproval).await;
}

// 검증: C10
#[tokio::test]
async fn grant_with_another_fingerprint_places_nothing() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let real = bench.grant(id, 10, "70000", true);
    let forged = ApprovalGrant::new(real.id(), "0".repeat(64));

    refused(&bench, id, forged, TradeState::AwaitingApproval).await;
}

// 검증: C10
#[tokio::test]
async fn grant_for_other_content_places_nothing() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let grant = bench.grant(id, 10, "71000", true);

    refused(&bench, id, grant, TradeState::AwaitingApproval).await;
}

// 검증: C10
#[tokio::test]
async fn grant_not_yet_consumed_places_nothing() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let grant = bench.grant(id, 10, "70000", false);

    refused(&bench, id, grant, TradeState::AwaitingApproval).await;
}

// 검증: C10
#[tokio::test]
async fn made_up_grant_places_nothing_even_without_waiting() {
    let bench = Bench::new().await;
    let waiting = awaiting(&bench);
    let judged = bench.judged();

    refused(
        &bench,
        waiting,
        ApprovalGrant::new("a1b2", "c3d4"),
        TradeState::AwaitingApproval,
    )
    .await;
    refused(
        &bench,
        judged,
        ApprovalGrant::new("a1b2", "c3d4"),
        TradeState::Judged,
    )
    .await;
}

// 검증: C10
#[tokio::test]
async fn sell_over_the_remaining_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.holding().await;

    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "11", "71000"))
        .await;

    assert!(
        matches!(result, Err(Error::SellTooLarge { remaining }) if remaining == d("10")),
        "{result:?}"
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert!(bench.exec.pending_sells().is_empty());
    assert_eq!(bench.trade(id).state, TradeState::Holding);
}

// 검증: C10
#[tokio::test]
async fn sell_counts_only_its_own_trade() {
    let bench = Bench::new().await;
    let first = bench.holding().await;
    let _second = bench.holding().await;

    let result = bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "15", "71000"))
        .await;

    assert!(
        matches!(result, Err(Error::SellTooLarge { remaining }) if remaining == d("10")),
        "{result:?}"
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());

    bench
        .exec
        .sell(bench.sell(first, SellPurpose::TakeProfit, "10", "71000"))
        .await
        .unwrap();
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: C10 E29
#[tokio::test]
async fn sell_after_a_partial_sale_counts_what_is_left() {
    let bench = Bench::new().await;
    let id = bench.holding().await;
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "4", "70000"))
        .await
        .unwrap();
    bench.set_at(bench.now() + chrono::Duration::seconds(1));
    bench.exec.on_quote(&bench.quote("70000")).unwrap();
    assert!(bench.orders(OrderGroup::Open).await.is_empty());

    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "7", "71000"))
        .await;

    assert!(
        matches!(result, Err(Error::SellTooLarge { remaining }) if remaining == d("6")),
        "{result:?}"
    );
    bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, "6", "71000"))
        .await
        .unwrap();
}
