//! 6차: 답 없는 사기를 줄여 내기(E22 · FN2 3b3). 알림이 갔는데 답이 없어 닫힌 사는 요청만, 요청한
//! 수량 · 금액 안에서, 한 번만 낸다. 실행기는 부르는 쪽의 말을 믿지 않고 저장소의 승인 기록과 대조한다.

pub(crate) mod desk;

use pi_approval::{
    ApprovalNumber, Decision, DueAction, DueHandling, Notice, OrderSubject, Side as AskSide,
    SignalKind,
};
use pi_exec::{Error, ReducedSubmission, Submission};
use pi_market::{Market, Symbol};
use pi_paper::{AccountKind, ApprovedOrder, BrokerError, Fault, OrderGroup, OrderType, Side};
use pi_rules::ApprovalGrant;
use pi_store::{
    AccountKind as StoreAccount, Amount, ApprovalStatus, BlockKind, StoreError, TradeId, TradeState,
};

use crate::common::{Bench, STOP, config, d, guard};
use desk::{
    VALID, WAIT, approvals, ask, awaiting, buying, close_at, nothing_sent, reduced, todo_of,
    unanswered, unanswered_with, untouched, view, wait_for_approval,
};

/// 알림이 갔다.
const SENT: Option<Notice> = Some(Notice::Sent);

/// 그 번호로 4주 70,000원을 내 보면 `UnansweredMismatch`이고, 매매는 승인대기 그대로 · 주문 없음.
async fn refuses(bench: &Bench, id: TradeId, number: &ApprovalNumber) {
    let result = bench
        .exec
        .submit_reduced(reduced(bench, id, number, "4", "70000"))
        .await;

    assert!(
        matches!(result, Err(Error::UnansweredMismatch)),
        "{result:?}"
    );
    untouched(bench, id).await;
}

/// 열린 주문이 하나이고 그 수량 · 값의 삼성전자 사기다.
async fn one_open_buy(bench: &Bench, quantity: &str, price: &str) {
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].side, Side::Buy);
    assert_eq!(open[0].quantity, d(quantity));
    assert_eq!(open[0].price, Some(d(price)));
}

// ── 정상 ──────────────────────────────────────────────────────────────────

// 검증: E22 C10 V6
#[tokio::test]
async fn unanswered_buy_is_ordered_with_the_reduced_quantity() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::ReduceWithinLimit));

    let order_id = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    let open = bench.orders(OrderGroup::Open).await;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].order_id, order_id);
    assert_eq!(open[0].side, Side::Buy);
    assert_eq!(open[0].quantity, d("4"));
    assert_eq!(open[0].price, Some(d("70000")));
    assert!(bench.snapshot_saved());
}

// 검증: E22
#[tokio::test]
async fn approval_record_is_only_read() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    // 답 없음 · 소비 안 됨 그대로이고, 처리 기록은 부르는 쪽이 남길 때까지 없다.
    let seen = view(&bench, &number);
    assert_eq!(seen.status, ApprovalStatus::NoAnswer);
    assert!(!seen.consumed);
    assert_eq!(approvals(&bench).due_handling(&number).unwrap(), None);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::ReduceWithinLimit));
}

// 검증: E22 C10
#[tokio::test]
async fn reduced_order_fills_into_a_holding_of_the_reduced_quantity() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    bench.set(WAIT + 1);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert_eq!(bench.exec.guards(), vec![guard(id, "4")]);
    assert_eq!(bench.holdings().await[0].quantity, d("4"));
}

// ── 수량 경계: 0 < 표시 수량 ≤ 요청 수량 ──────────────────────────────────

// 검증: E22
#[tokio::test]
async fn quantity_equal_to_the_request_is_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "10", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "10", "70000").await;
}

// 검증: E22
#[tokio::test]
async fn quantity_one_over_the_request_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    // 금액(11 × 63,000 = 693,000)은 요청 금액(700,000) 안이어도 수량이 요청보다 많다.
    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "11", "63000"))
        .await;

    assert!(
        matches!(result, Err(Error::UnansweredMismatch)),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// 검증: E22
#[tokio::test]
async fn zero_quantity_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "0", "70000"))
        .await;

    assert!(
        matches!(result, Err(Error::UnansweredMismatch)),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// ── 금액 경계: 표시 수량 × 지정가 ≤ 요청 수량 × 요청 값 (6차 조정자 결정 2번) ──

// 검증: E22
#[tokio::test]
async fn amount_equal_to_the_request_is_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    // 요청 10 × 49,000 = 490,000. 줄인 주문 7 × 70,000 = 490,000.
    let number = unanswered(&bench, id, 10, "49000", SENT);

    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "7", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "7", "70000").await;
}

// 검증: E22
#[tokio::test]
async fn amount_one_over_the_request_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    // 요청 10 × 48,999.9 = 489,999. 줄인 주문 7 × 70,000 = 490,000(1 큼).
    let number = unanswered(&bench, id, 10, "48999.9", SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "7", "70000"))
        .await;

    assert!(
        matches!(result, Err(Error::UnansweredMismatch)),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// 검증: E22
#[tokio::test]
async fn same_quantity_at_a_higher_price_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    // 수량은 요청과 같아도 지금 시세로 다시 셈한 값이 올라 금액(701,000)이 요청(700,000)을 넘는다.
    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "10", "70100"))
        .await;

    assert!(
        matches!(result, Err(Error::UnansweredMismatch)),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// 검증: E22
#[tokio::test]
async fn fewer_shares_at_a_higher_price_within_the_amount_are_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    // 값은 대조하지 않는다: 9 × 77,700 = 699,300 ≤ 700,000.
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "9", "77700"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "9", "77700").await;
}

// ── 한 번만 ───────────────────────────────────────────────────────────────

// 검증: E22 C10
#[tokio::test]
async fn second_call_with_the_same_number_orders_nothing_more() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    let again = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(again, Err(Error::NotAwaitingApproval(TradeState::Ordered))),
        "{again:?}"
    );
    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "4", "70000").await;
    assert!(bench.orders(OrderGroup::Closed).await.is_empty());
}

// 검증: E22
#[tokio::test]
async fn same_number_after_the_fill_orders_nothing_more() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();
    bench.set(WAIT + 1);
    bench.exec.on_quote(&bench.quote("70000")).unwrap();

    let again = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(again, Err(Error::NotAwaitingApproval(TradeState::Holding))),
        "{again:?}"
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert_eq!(bench.orders(OrderGroup::Closed).await.len(), 1);
    assert_eq!(bench.holdings().await[0].quantity, d("4"));
}

// 검증: E22
#[tokio::test]
async fn number_with_a_handling_record_is_refused_even_for_an_awaiting_trade() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    approvals(&bench)
        .record_due_handling(&number, DueHandling::Reduced { quantity: 4 })
        .unwrap();
    assert_eq!(todo_of(&bench, &number), None);

    refuses(&bench, id, &number).await;
}

// ── 파는 주문 ─────────────────────────────────────────────────────────────

// 검증: E22 E33
#[tokio::test]
async fn sell_permit_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let submission = ReducedSubmission {
        permit: bench.permit(id, Side::Sell, "4", "70000"),
        ..reduced(&bench, id, &number, "4", "70000")
    };

    let result = bench.exec.submit_reduced(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    untouched(&bench, id).await;
}

// 검증: E22
#[tokio::test]
async fn unanswered_sell_request_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let selling = OrderSubject {
        side: AskSide::Sell,
        signal_kind: SignalKind::StopLoss,
        ..buying(&bench, id, 10, "70000")
    };
    let number = unanswered_with(&bench, selling, SENT);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::TeamDecides));

    refuses(&bench, id, &number).await;
}

// ── 다른 매매 · 다른 종목 ─────────────────────────────────────────────────

// 검증: E22
#[tokio::test]
async fn number_of_another_trade_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let other = awaiting(&bench);
    let number = unanswered(&bench, other, 10, "70000", SENT);

    refuses(&bench, id, &number).await;
    assert_eq!(bench.trade(other).state, TradeState::AwaitingApproval);
}

// 검증: E22 E33
#[tokio::test]
async fn permit_tied_to_another_trade_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let other = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let submission = ReducedSubmission {
        permit: bench.permit(other, Side::Buy, "4", "70000"),
        ..reduced(&bench, id, &number, "4", "70000")
    };

    let result = bench.exec.submit_reduced(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    untouched(&bench, id).await;
    assert_eq!(bench.trade(other).state, TradeState::AwaitingApproval);
}

// 검증: E22 E33
#[tokio::test]
async fn unbound_permit_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let submission = ReducedSubmission {
        permit: bench.unbound_permit(Side::Buy, "4", "70000"),
        ..reduced(&bench, id, &number, "4", "70000")
    };

    let result = bench.exec.submit_reduced(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    untouched(&bench, id).await;
}

// 검증: E22 E33
#[tokio::test]
async fn permit_for_another_symbol_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let other = ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new("000660"),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d("4"),
        price: Some(d("70000")),
        account_kind: AccountKind::Paper,
    };
    let submission = ReducedSubmission {
        permit: bench.authority.approve_for_trade(id.0, other),
        ..reduced(&bench, id, &number, "4", "70000")
    };

    let result = bench.exec.submit_reduced(submission).await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    untouched(&bench, id).await;
}

// 검증: E22
#[tokio::test]
async fn request_for_another_symbol_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let elsewhere = OrderSubject {
        symbol: "000660".to_string(),
        ..buying(&bench, id, 10, "70000")
    };
    let number = unanswered_with(&bench, elsewhere, SENT);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::ReduceWithinLimit));

    refuses(&bench, id, &number).await;
}

// 검증: E22 E25
#[tokio::test]
async fn request_for_another_account_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let live = OrderSubject {
        account: StoreAccount::Live,
        ..buying(&bench, id, 10, "70000")
    };
    let number = unanswered_with(&bench, live, SENT);

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn market_permit_differs_from_the_limit_request() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let market = ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(crate::common::SAM),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: d("4"),
        price: None,
        account_kind: AccountKind::Paper,
    };
    let submission = ReducedSubmission {
        permit: bench.authority.approve_for_trade(id.0, market),
        ..reduced(&bench, id, &number, "4", "70000")
    };

    let result = bench.exec.submit_reduced(submission).await;

    assert!(
        matches!(result, Err(Error::UnansweredMismatch)),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// ── 요청의 결과: 답 없음(소비 안 됨)만 받는다 ─────────────────────────────

// 검증: E22
#[tokio::test]
async fn request_still_waiting_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = ask(&bench, buying(&bench, id, 10, "70000"), SENT).number;
    close_at(&bench, WAIT - 1);
    assert_eq!(view(&bench, &number).status, ApprovalStatus::Pending);

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn request_past_its_wait_but_not_closed_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = ask(&bench, buying(&bench, id, 10, "70000"), SENT).number;
    // 기다릴 시간은 지났지만 아무도 답 없음으로 닫지 않았다. 실행기가 스스로 닫지 않는다.
    bench.set(WAIT);

    refuses(&bench, id, &number).await;
    assert_eq!(view(&bench, &number).status, ApprovalStatus::Pending);
}

// 검증: E22
#[tokio::test]
async fn approved_request_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let grant = bench.grant(id, 10, "70000", false);
    let number = ApprovalNumber::new(grant.id());
    assert_eq!(view(&bench, &number).status, ApprovalStatus::Approved);

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn approved_and_consumed_request_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let grant = bench.grant(id, 10, "70000", true);
    let number = ApprovalNumber::new(grant.id());
    assert!(view(&bench, &number).consumed);

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn rejected_request_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let requested = ask(&bench, buying(&bench, id, 10, "70000"), SENT);
    approvals(&bench)
        .answer(
            &requested.number,
            requested.fingerprint.as_str(),
            Decision::Reject,
        )
        .unwrap();
    close_at(&bench, WAIT);
    assert_eq!(
        view(&bench, &requested.number).status,
        ApprovalStatus::Rejected
    );

    refuses(&bench, id, &requested.number).await;
}

// 검증: E22
#[tokio::test]
async fn request_closed_after_its_signal_validity_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = ask(&bench, buying(&bench, id, 10, "70000"), SENT).number;
    close_at(&bench, VALID);
    assert_eq!(view(&bench, &number).status, ApprovalStatus::Expired);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::Dropped));

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn unanswered_request_past_its_signal_validity_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    // 답 없음으로 닫힌 뒤 내기 전에 신호 유효 시간이 지났다.
    bench.set(VALID);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::Dropped));

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn unanswered_request_just_inside_its_signal_validity_is_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench.set(VALID - 1);

    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "4", "70000").await;
}

// 검증: E22
#[tokio::test]
async fn unknown_number_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    unanswered(&bench, id, 10, "70000", SENT);
    let unknown = ApprovalNumber::new("0".repeat(32));

    refuses(&bench, id, &unknown).await;
}

// ── 알림(P20): 알림이 갔다는 기록이 있어야 한다 ───────────────────────────

// 검증: E22
#[tokio::test]
async fn failed_notice_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", Some(Notice::Failed));
    assert_eq!(todo_of(&bench, &number), Some(DueAction::DoNotBuy));

    refuses(&bench, id, &number).await;
}

// 검증: E22
#[tokio::test]
async fn missing_notice_record_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", None);
    assert_eq!(todo_of(&bench, &number), Some(DueAction::DoNotBuy));

    refuses(&bench, id, &number).await;
}

// ── 매매 상태 · 매매 수량 ─────────────────────────────────────────────────

// 검증: E22 C10
#[tokio::test]
async fn judged_trade_is_not_taken_on_this_path() {
    let bench = Bench::new().await;
    let id = bench.judged();
    let number = unanswered(&bench, id, 10, "70000", SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(result, Err(Error::NotAwaitingApproval(TradeState::Judged))),
        "{result:?}"
    );
    assert_eq!(bench.trade(id).state, TradeState::Judged);
    nothing_sent(&bench).await;
}

// 검증: E22
#[tokio::test]
async fn permit_below_the_stored_trade_quantity_is_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    bench.store.set_trade_quantity(id, 10).unwrap();
    let number = unanswered(&bench, id, 10, "70000", SENT);

    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    one_open_buy(&bench, "4", "70000").await;
    // 실행기는 매매 수량을 고쳐 쓰지 않는다.
    assert_eq!(bench.store.trade_quantity(id).unwrap(), Some(10));
}

// 검증: E22
#[tokio::test]
async fn permit_equal_to_the_stored_trade_quantity_is_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    bench.store.set_trade_quantity(id, 4).unwrap();
    let number = unanswered(&bench, id, 10, "70000", SENT);

    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "4", "70000").await;
}

// 검증: E22 E33
#[tokio::test]
async fn permit_above_the_stored_trade_quantity_is_refused() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    bench.store.set_trade_quantity(id, 3).unwrap();
    let number = unanswered(&bench, id, 10, "70000", SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(matches!(result, Err(Error::PermitMismatch)), "{result:?}");
    untouched(&bench, id).await;
}

// 검증: E22 C10
#[tokio::test]
async fn incomplete_rationale_places_nothing() {
    let bench = Bench::new().await;
    let id = bench.judged_with(StoreAccount::Paper, None);
    wait_for_approval(&bench, id);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(
            result,
            Err(Error::Store(StoreError::TransitionDenied { .. }))
        ),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// ── 계좌(E25) · 준비(E16) ─────────────────────────────────────────────────

// 검증: E22 E25
#[tokio::test]
async fn live_template_is_never_ordered() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let submission = ReducedSubmission {
        template_account: AccountKind::Live,
        ..reduced(&bench, id, &number, "4", "70000")
    };

    let result = bench.exec.submit_reduced(submission).await;

    assert!(
        matches!(
            result,
            Err(Error::AccountKindMismatch {
                broker: AccountKind::Paper,
                found: AccountKind::Live,
            })
        ),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// 검증: E22 E25
#[tokio::test]
async fn live_trade_is_never_ordered() {
    let bench = Bench::new().await;
    let id = bench.judged_with(StoreAccount::Live, Some(Amount(STOP)));
    wait_for_approval(&bench, id);
    let live = OrderSubject {
        account: StoreAccount::Live,
        ..buying(&bench, id, 10, "70000")
    };
    let number = unanswered_with(&bench, live, SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(
            result,
            Err(Error::AccountKindMismatch {
                broker: AccountKind::Paper,
                found: AccountKind::Live,
            })
        ),
        "{result:?}"
    );
    untouched(&bench, id).await;
}

// 검증: E22 E16
#[tokio::test]
async fn nothing_is_taken_before_reconciling() {
    let bench = Bench::unready(config());
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(matches!(result, Err(Error::NotReady)), "{result:?}");
    untouched(&bench, id).await;
}

// ── 다시 켠 뒤 ────────────────────────────────────────────────────────────

// 검증: E22 E16
#[tokio::test]
async fn after_restart_it_is_ordered_once_reconciled() {
    let mut bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench.reopen(config());

    let early = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;
    assert!(matches!(early, Err(Error::NotReady)), "{early:?}");
    untouched(&bench, id).await;

    // 맞춰 보기는 승인대기 매매를 건드리지 않고, 알림 · 답 없음 기록은 저장소에 남아 있다.
    bench.exec.reconcile().await.unwrap();
    assert_eq!(bench.trade(id).state, TradeState::AwaitingApproval);
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "4", "70000").await;
}

// 검증: E22 E16
#[tokio::test]
async fn after_restart_an_ordered_trade_is_not_ordered_again() {
    let mut bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();

    // 주문은 나갔는데 처리 기록을 남기기 전에 꺼졌다: 그 번호는 아직 "줄여서 낼 것"으로 보인다.
    bench.reopen(config());
    bench.exec.reconcile().await.unwrap();
    assert_eq!(todo_of(&bench, &number), Some(DueAction::ReduceWithinLimit));
    let again = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(again, Err(Error::NotAwaitingApproval(TradeState::Ordered))),
        "{again:?}"
    );
    one_open_buy(&bench, "4", "70000").await;
}

// ── 증권사: 큰 금액 확인 · 장애 ───────────────────────────────────────────

/// 국내 큰 금액 확인 기준이 280,000원인 설정.
fn confirm_from_280k() -> pi_paper::PaperConfig {
    let mut config = config();
    config.kr.high_value_confirm_from = Some(d("280000"));
    config
}

// 검증: E22 E33
#[tokio::test]
async fn reduced_order_never_carries_the_high_value_confirmation() {
    let bench = Bench::with(confirm_from_280k()).await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    // 4 × 70,000 = 280,000: 확인 기준 이상인데 확인 표시가 없어 증권사가 받지 않는다.
    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(
            result,
            Err(Error::Rejected(
                BrokerError::ConfirmHighValueRequired { .. }
            ))
        ),
        "{result:?}"
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    let trade = bench.trade(id);
    assert_eq!(trade.state, TradeState::Blocked);
    let reason = trade.blocked.unwrap();
    assert_eq!(reason.kind, BlockKind::UnfilledCancelled);
    assert!(!reason.detail.trim().is_empty());
}

// 검증: E22
#[tokio::test]
async fn reduced_order_below_the_high_value_threshold_is_ordered() {
    let bench = Bench::with(confirm_from_280k()).await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    // 3 × 70,000 = 210,000: 확인 기준 아래다.
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "3", "70000"))
        .await
        .unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    one_open_buy(&bench, "3", "70000").await;
}

// 검증: E22 E13
#[tokio::test]
async fn broker_refusal_is_an_anomaly_stop() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench.exec.broker().set_fault(Some(Fault::RejectOrders));

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(result, Err(Error::AnomalyStop(BrokerError::Unavailable))),
        "{result:?}"
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    let trade = bench.trade(id);
    assert_eq!(trade.state, TradeState::Blocked);
    let reason = trade.blocked.unwrap();
    assert_eq!(reason.kind, BlockKind::UnfilledCancelled);
    assert!(!reason.detail.trim().is_empty());
}

// 검증: E22 E13
#[tokio::test]
async fn broker_silence_is_an_anomaly_stop() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    bench.exec.broker().set_fault(Some(Fault::NoResponse));

    let result = bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await;

    assert!(
        matches!(result, Err(Error::AnomalyStop(BrokerError::NoResponse))),
        "{result:?}"
    );
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert_eq!(bench.trade(id).state, TradeState::Blocked);
    assert!(bench.exec.guards().is_empty());
}

// ── 옛 길은 그대로 ────────────────────────────────────────────────────────

// 검증: E22 C10
#[tokio::test]
async fn grant_made_from_an_unanswered_number_is_refused_by_submit() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let requested = ask(&bench, buying(&bench, id, 10, "70000"), SENT);
    close_at(&bench, WAIT);
    assert_eq!(
        view(&bench, &requested.number).status,
        ApprovalStatus::NoAnswer
    );
    let submission = Submission {
        approval: Some(ApprovalGrant::new(
            requested.number.as_str(),
            requested.fingerprint.as_str(),
        )),
        ..bench.buy(id, "10", "70000")
    };

    let result = bench.exec.submit(submission).await;

    assert!(matches!(result, Err(Error::ApprovalMismatch)), "{result:?}");
    untouched(&bench, id).await;
}
