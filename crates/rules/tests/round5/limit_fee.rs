//! 1 · 2. 사는 주문의 금액 한도 판정에 수수료를 넣는다(E21 · E22, 결정 14).
//!
//! 한도와 비교하는 금액 = `수량 × 진입가 + 올림(수량 × 진입가 × 수수료율)`(원은 1원 단위로 올림).
//! 보이는 금액(`Passed::amount` · `PendingApproval::amount`)은 `수량 × 진입가` 그대로다.
//! 기본 장면: 평가액 10,000,000원 · 시세 50,000원 · 수수료율 0.015% · 위험으로 정한 수량 33주.

use pi_paper::{OrderType, Side};
use pi_rules::reduce_within_limit;
use pi_rules::{BlockKind, Outcome, PendingApproval, Principle, Reason, Violation};

use crate::common::{Scenario, approved, d, expect_pass, kr_buy, kr_buy_qty, kr_rules_at};

fn expect_pending(outcome: Outcome) -> PendingApproval {
    match outcome {
        Outcome::NeedsApproval(pending) => pending,
        other => panic!("승인을 기다려야 한다: {other:?}"),
    }
}

/// 한도(원)를 정한 국내 장면.
fn with_limit(won: &str) -> Scenario {
    let mut s = Scenario::kr();
    s.set_limit_won(d(won));
    s
}

// 검증: E21 V25
#[test]
fn a_buy_whose_quantity_times_price_equals_the_limit_waits_for_approval() {
    // 20 × 50,000 = 1,000,000 = 한도. 수수료 150원을 더하면 한도를 넘는다(전에는 통과).
    let s = with_limit("1000000");
    let pending = expect_pending(s.check(&kr_buy_qty("20")));
    assert_eq!(
        pending.order(),
        &approved(Side::Buy, OrderType::Limit, "20", Some("50000"))
    );
    // 보이는 금액은 수량 × 값 그대로.
    assert_eq!(pending.amount(), d("1000000"));
    assert_eq!(pending.limit(), d("1000000"));
}

// 검증: E21 V25
#[test]
fn a_buy_whose_cost_with_fee_fits_the_limit_passes_and_shows_the_plain_amount() {
    // 19 × 50,000 = 950,000, 수수료 올림(142.5) = 143 → 950,143 ≤ 1,000,000.
    let s = with_limit("1000000");
    let passed = expect_pass(s.check(&kr_buy_qty("19")));
    assert_eq!(
        passed.order(),
        &approved(Side::Buy, OrderType::Limit, "19", Some("50000"))
    );
    assert_eq!(passed.amount(), d("950000"));
}

// 검증: E21 V25
#[test]
fn the_limit_is_inclusive_of_the_cost_with_fee() {
    // 20주: 1,000,000 + 150 = 1,000,150. 한도가 그와 같으면 통과, 1원 모자라면 승인대기.
    let s = with_limit("1000150");
    let passed = expect_pass(s.check(&kr_buy_qty("20")));
    assert_eq!(passed.amount(), d("1000000"));

    let s = with_limit("1000149");
    let pending = expect_pending(s.check(&kr_buy_qty("20")));
    assert_eq!(pending.amount(), d("1000000"));
    assert_eq!(pending.limit(), d("1000149"));
}

// 검증: E21 V25
#[test]
fn the_fee_share_is_rounded_up_to_the_won() {
    // 수수료율 0.01002%: 19주 950,000 × 0.0001002 = 95.19 → 올림 96. 반올림이면 95라 갈린다.
    let mut s = with_limit("950096");
    s.rules = kr_rules_at(d("0.0001002"));
    let passed = expect_pass(s.check(&kr_buy_qty("19")));
    assert_eq!(passed.amount(), d("950000"));

    let mut s = with_limit("950095");
    s.rules = kr_rules_at(d("0.0001002"));
    expect_pending(s.check(&kr_buy_qty("19")));
}

// 검증: E21 V25
#[test]
fn a_zero_fee_rate_keeps_the_plain_comparison() {
    // 수수료가 0이면 수량 × 값이 한도와 같을 때 지난다(넘을 때만 기다린다).
    let mut s = with_limit("1000000");
    s.rules = kr_rules_at(d("0"));
    let passed = expect_pass(s.check(&kr_buy_qty("20")));
    assert_eq!(passed.amount(), d("1000000"));
    s.set_limit_won(d("999999"));
    expect_pending(s.check(&kr_buy_qty("20")));
}

// 검증: E22 E21 P12
#[test]
fn an_unanswered_buy_is_cut_so_that_the_cost_with_fee_fits_the_limit() {
    // 수수료율 0.5%, 한도 1,050,000: 전엔 floor(1,050,000 ÷ 50,000) = 21주.
    // 21주는 1,050,000 + 5,250이라 넘고, 20주는 1,000,000 + 5,000 = 1,005,000 ≤ 한도.
    let mut s = with_limit("1050000");
    s.rules = kr_rules_at(d("0.005"));
    let pending = expect_pending(s.check(&kr_buy()));
    assert_eq!(pending.order().quantity, d("33"));
    let passed = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap();
    assert_eq!(
        passed.order(),
        &approved(Side::Buy, OrderType::Limit, "20", Some("50000"))
    );
    assert_eq!(passed.amount(), d("1000000"));
    assert!(passed.approval().is_none());
}

// 검증: E22 E21 P12 V24
#[test]
fn nothing_fits_when_one_share_with_its_fee_is_over_the_limit() {
    // 한도 50,000: 한 주 50,000 + 수수료 올림(7.5) 8 = 50,008 > 한도라 한 주도 못 산다(전엔 1주).
    let s = with_limit("50000");
    let pending = expect_pending(s.check(&kr_buy()));
    let blocked = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap_err();
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
    assert_eq!(
        blocked.reason,
        Reason::Violations(vec![Violation::NothingWithinLimit])
    );
    assert_eq!(Violation::NothingWithinLimit.principle(), Principle::P12);
}

// 검증: E22 E21
#[test]
fn one_share_is_kept_when_its_fee_fits_exactly() {
    // 한도 50,008 = 50,000 + 8 → 한 주.
    let s = with_limit("50008");
    let pending = expect_pending(s.check(&kr_buy()));
    let passed = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap();
    assert_eq!(passed.order().quantity, d("1"));
    assert_eq!(passed.amount(), d("50000"));
}

// 검증: E21 E34
#[test]
fn a_sell_is_not_held_to_the_limit_with_fee() {
    // 파는 주문은 지금 그대로다: 승인을 기다리지 않는다(한도 1%에서도).
    use pi_rules::Purpose;
    let mut s = with_limit("100000");
    s.book.positions.push(pi_rules::Position {
        market: pi_market::Market::Kr,
        symbol: pi_market::Symbol::new(crate::common::SAMSUNG),
        quantity: d("10"),
        avg_price: d("55000"),
        last_price: d("50000"),
    });
    let draft = pi_rules::OrderDraft {
        purpose: Purpose::StopLoss,
        order_type: OrderType::Market,
        limit_price: None,
        ..kr_buy()
    };
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.amount(), d("500000"));
}
