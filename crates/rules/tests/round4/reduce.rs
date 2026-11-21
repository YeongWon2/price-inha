//! 4. 답 없는 사기를 금액 한도 안으로 줄여 다시 점검(FN2 3b3 · E22).

use pi_market::Symbol;
use pi_paper::{OrderType, Side};
use pi_rules::{
    BlockKind, OrderDraft, Outcome, PendingApproval, Principle, Purpose, Reason, TeamStatus,
    Violation, reduce_within_limit,
};

use crate::common::{HYNIX, SAMSUNG, Scenario, approved, d, kr_buy, kr_sell, violations};

/// 자동 주문 금액 한도를 10%(1,000,000원)로 낮춘 국내 장면에서 승인을 기다리는 주문을 얻는다.
/// 수량: 1회 위험 100,000 ÷ (50,000 − 47,000) = 33, 종목 비중 2,000,000 ÷ 50,000 = 40 → 33.
/// 금액 33 × 50,000 = 1,650,000 > 1,000,000.
fn pending_for(s: &Scenario, draft: &OrderDraft) -> PendingApproval {
    match s.check(draft) {
        Outcome::NeedsApproval(pending) => pending,
        other => panic!("승인을 기다려야 한다: {other:?}"),
    }
}

fn pending_scenario() -> (Scenario, PendingApproval) {
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("10");
    let pending = pending_for(&s, &kr_buy());
    assert_eq!(pending.order().quantity, d("33"));
    (s, pending)
}

// 검증: E22 E21 P12
#[test]
fn an_unanswered_buy_is_cut_down_to_the_auto_order_limit() {
    // floor(1,000,000 ÷ 50,000) = 20주, 금액 20 × 50,000 = 1,000,000 = 한도(이하라 통과).
    let (s, pending) = pending_scenario();
    let passed = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap();
    assert_eq!(
        passed.order(),
        &approved(Side::Buy, OrderType::Limit, "20", Some("50000"))
    );
    assert_eq!(passed.amount(), d("1000000"));
    assert!(passed.amount() <= d("1000000"));
    assert!(passed.approval().is_none());
}

// 검증: E22 E21 P7
#[test]
fn the_pending_quantity_caps_the_reduced_order_even_when_more_would_fit() {
    // 그 사이 시세가 49,000으로 내리고 한도가 20%(2,000,000)로 넓어졌다.
    // 다시 셈: 100,000 ÷ 2,000 = 50, 2,000,000 ÷ 49,000 → 40 → 40주. 한도 안 floor = 40.
    // pending 33이 가장 작다. 지정가는 다시 셈한 49,000(호가 단위 50원에 맞다).
    let (mut s, pending) = pending_scenario();
    s.quote.price = d("49000");
    s.params.auto_order_limit_pct = d("20");
    let passed = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap();
    assert_eq!(
        passed.order(),
        &approved(Side::Buy, OrderType::Limit, "33", Some("49000"))
    );
    assert_eq!(passed.amount(), d("1617000"));
    assert!(passed.approval().is_none());
}

// 검증: E22 P7
#[test]
fn a_smaller_fresh_quantity_wins_after_the_quote_fell() {
    // 시세 49,000, AI 수량 12주 → 다시 셈한 수량 min(12, 40) = 12 < pending 33 < 한도 안 20.
    let (mut s, pending) = pending_scenario();
    s.quote.price = d("49000");
    let draft = OrderDraft {
        ai_quantity: Some(d("12")),
        ..kr_buy()
    };
    let passed = reduce_within_limit(pending, &draft, &s.ctx()).unwrap();
    assert_eq!(
        passed.order(),
        &approved(Side::Buy, OrderType::Limit, "12", Some("49000"))
    );
    assert_eq!(passed.amount(), d("588000"));
}

// 검증: E22 E31 V26
#[test]
fn a_team_that_halted_meanwhile_blocks_the_reduced_buy() {
    let (mut s, pending) = pending_scenario();
    s.team = TeamStatus::DailyLimitHalt;
    let blocked = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap_err();
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
    assert_eq!(
        blocked.reason,
        Reason::TeamNotOperating(TeamStatus::DailyLimitHalt)
    );
}

// 검증: E22 V26
#[test]
fn another_symbol_or_a_sell_is_not_the_order_that_was_waiting() {
    // 다른 종목: 하이닉스 시세로 하이닉스를 사면 점검은 지나지만 기다리던 주문이 아니다.
    let (mut s, pending) = pending_scenario();
    s.quote.symbol = Symbol::new(HYNIX);
    let draft = OrderDraft {
        symbol: Symbol::new(HYNIX),
        ..kr_buy()
    };
    let blocked = reduce_within_limit(pending, &draft, &s.ctx()).unwrap_err();
    assert_eq!(blocked.reason, Reason::ApprovalMismatch);
    assert!(!blocked.detail().trim().is_empty());

    // 파는 주문: 손절은 점검을 지나도 사는 주문이 아니라 같은 막힘.
    let (mut s, pending) = pending_scenario();
    s.hold(SAMSUNG, "10", "45000", "50000");
    let blocked = reduce_within_limit(pending, &kr_sell(Purpose::StopLoss), &s.ctx()).unwrap_err();
    assert_eq!(blocked.reason, Reason::ApprovalMismatch);
}

// 검증: E22 P12 V24
#[test]
fn nothing_fits_within_the_limit_is_a_p12_block() {
    // 한도 0.4% = 40,000원 < 50,000원 → 한 주도 못 산다.
    let (mut s, pending) = pending_scenario();
    s.params.auto_order_limit_pct = d("0.4");
    let blocked = reduce_within_limit(pending, &kr_buy(), &s.ctx()).unwrap_err();
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
    assert_eq!(
        blocked.reason,
        Reason::Violations(vec![Violation::NothingWithinLimit])
    );
    assert_eq!(Violation::NothingWithinLimit.principle(), Principle::P12);
    assert!(blocked.detail().contains("P12"), "{}", blocked.detail());
}

// 검증: E22 E21 P12
#[test]
fn a_market_buy_is_still_blocked_over_the_auto_limit() {
    let (s, pending) = pending_scenario();
    let draft = OrderDraft {
        order_type: OrderType::Market,
        limit_price: None,
        ..kr_buy()
    };
    let blocked = reduce_within_limit(pending, &draft, &s.ctx()).unwrap_err();
    assert!(violations(&blocked).contains(&Violation::MarketBuyOverAutoLimit));
}
