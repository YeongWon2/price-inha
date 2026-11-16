//! 사는 주문 내기: 저장 먼저 · 계좌 종류 · 표시 · 승인 · 증권사 실패.

use pi_exec::{Error, Submission};
use pi_market::{Market, Symbol};
use pi_paper::{
    AccountKind, ApprovedOrder, BrokerError, Fault, OrderGroup, OrderType, PaperBroker, Side,
};
use pi_store::{
    AccountKind as StoreAccount, BlockKind, NewTrade, StoreError, TradeState, UnixMillis,
};

use crate::common::{Bench, SAM, approved, config, d};

// 검증: C10
#[tokio::test]
async fn submit_saves_ordered_then_places_the_order() {
    let bench = Bench::new().await;
    let id = bench.judged();

    let order_id = bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order_id, order_id);
    assert_eq!(open[0].side, Side::Buy);
    assert_eq!(open[0].quantity, d("10"));
    assert_eq!(open[0].price, Some(d("70000")));
    assert_eq!(open[0].symbol, Symbol::new(SAM));
}

// 검증: C10 V6
#[tokio::test]
async fn submit_saves_the_account_snapshot() {
    let bench = Bench::new().await;
    let id = bench.judged();

    bench
        .exec
        .submit(bench.buy(id, "10", "70000"))
        .await
        .unwrap();

    assert!(bench.snapshot_saved());
}

// 검증: E25
#[tokio::test]
async fn live_template_is_never_ordered() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let mut submission = bench.buy(id, "10", "70000");
    submission.template_account = AccountKind::Live;

    let result = bench.exec.submit(submission).await;

    assert!(matches!(
        result,
        Err(Error::AccountKindMismatch {
            broker: AccountKind::Paper,
            found: AccountKind::Live,
        })
    ));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert!(bench.orders(OrderGroup::Closed).await.is_empty());
}

// 검증: E25
#[tokio::test]
async fn live_trade_is_never_ordered_even_from_a_paper_template() {
    let bench = Bench::new().await;
    let id = bench.judged_with(StoreAccount::Live, Some(pi_store::Amount(65_000)));

    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;

    assert!(matches!(
        result,
        Err(Error::AccountKindMismatch {
            broker: AccountKind::Paper,
            found: AccountKind::Live,
        })
    ));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E33
#[tokio::test]
async fn permit_for_another_symbol_is_refused_before_saving() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let other = ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new("000660"),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d("10"),
        price: Some(d("30000")),
        account_kind: AccountKind::Paper,
    };
    let submission = Submission {
        permit: bench.authority.approve_for_trade(id.0, other),
        ..bench.buy(id, "10", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E33
#[tokio::test]
async fn sell_permit_on_the_buy_path_is_refused() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let submission = Submission {
        permit: bench.permit(id, Side::Sell, "10", "70000"),
        ..bench.buy(id, "10", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E33
#[tokio::test]
async fn permit_from_another_account_is_rejected_by_the_broker() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let (_other, foreign) = PaperBroker::open(config()).unwrap();
    let submission = Submission {
        permit: foreign.approve_for_trade(id.0, approved(Side::Buy, "10", "70000")),
        ..bench.buy(id, "10", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(
        result,
        Err(Error::Rejected(BrokerError::CheckInvalid))
    ));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    let trade = bench.trade(id);
    assert_eq!(trade.state, TradeState::Blocked);
    assert_eq!(
        trade.blocked.map(|b| b.kind),
        Some(BlockKind::UnfilledCancelled)
    );
}

// 검증: C10
#[tokio::test]
async fn awaiting_approval_without_grant_is_refused() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench
        .store
        .transition_trade(id, TradeState::AwaitingApproval, UnixMillis(1))
        .unwrap();

    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;

    assert!(matches!(result, Err(Error::ApprovalRequired)));
    assert_eq!(bench.trade(id).state, TradeState::AwaitingApproval);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: C10
#[tokio::test]
async fn awaiting_approval_with_grant_is_ordered() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench
        .store
        .transition_trade(id, TradeState::AwaitingApproval, UnixMillis(1))
        .unwrap();
    let submission = Submission {
        approval: Some(bench.grant(id, 10, "70000", true)),
        ..bench.buy(id, "10", "70000")
    };

    bench.exec.submit(submission).await.unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: C10
#[tokio::test]
async fn store_refusal_places_nothing() {
    let bench = Bench::new().await;
    let discussing = bench
        .store
        .create_trade(NewTrade {
            symbol: SAM.to_string(),
            account: StoreAccount::Paper,
            at: UnixMillis(1),
        })
        .unwrap();

    let result = bench
        .exec
        .submit(bench.buy(discussing.id, "10", "70000"))
        .await;

    assert!(matches!(result, Err(Error::Store(_))));
    assert_eq!(bench.trade(discussing.id).state, TradeState::Discussing);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert!(bench.orders(OrderGroup::Closed).await.is_empty());
}

// 검증: C10
#[tokio::test]
async fn incomplete_rationale_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.judged_with(StoreAccount::Paper, None);

    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;

    assert!(matches!(
        result,
        Err(Error::Store(StoreError::TransitionDenied { .. }))
    ));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}

// 검증: E13 C10
#[tokio::test]
async fn broker_refusal_is_an_anomaly_stop() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));

    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;

    assert!(matches!(
        result,
        Err(Error::AnomalyStop(BrokerError::Unavailable))
    ));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    let trade = bench.trade(id);
    assert_eq!(trade.state, TradeState::Blocked);
    let reason = trade.blocked.unwrap();
    assert_eq!(reason.kind, BlockKind::UnfilledCancelled);
    assert!(!reason.detail.trim().is_empty());
}

// 검증: E13
#[tokio::test]
async fn broker_silence_is_an_anomaly_stop() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench.exec.broker().set_fault(Some(Fault::NoResponse));

    let result = bench.exec.submit(bench.buy(id, "10", "70000")).await;

    assert!(matches!(
        result,
        Err(Error::AnomalyStop(BrokerError::NoResponse))
    ));
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert_eq!(bench.trade(id).state, TradeState::Blocked);
    assert!(bench.exec.guards().is_empty());
}

// 검증: E33
#[tokio::test]
async fn unbound_permit_is_refused_on_the_buy_path() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let submission = Submission {
        permit: bench.unbound_permit(Side::Buy, "10", "70000"),
        ..bench.buy(id, "10", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)));
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
}
