//! 한도 적용 기록: 점검이 수량을 줄였으면 앞 · 뒤 · 까닭을 한 줄씩 순서대로 남긴다.
//!
//! 기본 장면: 평가액 10,000,000원 · 시세 50,000원 · 손절 47,000원. 1회 위험 1% → 33주, 종목 20% →
//! 40주, 업종 40% → 80주, 자동 주문 20% = 2,000,000원.

use pi_paper::{OrderType, Side};
use pi_rules::{
    LimitClamp, LimitKind, OrderDraft, Outcome, PendingApproval, Purpose, TeamStatus, check,
    limit_clamps, reduce_within_limit, reduce_within_limit_recorded,
};

use crate::common::{HYNIX, SAMSUNG, Scenario, approved, d, expect_pass, kr_buy, kr_sell};

fn buy_with(ai_quantity: &str) -> OrderDraft {
    OrderDraft {
        ai_quantity: Some(d(ai_quantity)),
        ..kr_buy()
    }
}

fn line(limit: LimitKind, before: &str, after: &str) -> LimitClamp {
    LimitClamp {
        limit,
        before: d(before),
        after: d(after),
    }
}

/// 자동 주문 한도를 10%(1,000,000원)로 낮춘 장면에서 승인을 기다리는 주문을 얻는다.
fn pending_for(s: &Scenario, draft: &OrderDraft) -> PendingApproval {
    match s.check(draft) {
        Outcome::NeedsApproval(pending) => pending,
        other => panic!("승인을 기다려야 한다: {other:?}"),
    }
}

fn limited_scenario() -> Scenario {
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("10");
    s
}

// 검증: R4-4 C24 T31 P1 P7 E21
#[test]
fn nothing_is_recorded_when_no_limit_cut_the_quantity() {
    // AI 수량 10주는 어느 한도보다 작다 → 빈 목록. 같은 장면에서 AI 수량 100주는 1회 위험이 줄인다.
    let s = Scenario::kr();
    assert_eq!(limit_clamps(&buy_with("10"), &s.ctx()), Vec::new());
    assert_eq!(
        limit_clamps(&buy_with("100"), &s.ctx()),
        vec![line(LimitKind::RiskPerTrade, "100", "33")]
    );
    // 판정과 수량은 그대로다.
    let passed = expect_pass(s.check(&buy_with("10")));
    assert_eq!(passed.order().quantity, d("10"));
}

// 검증: R4-4 C24 T31 P1 P7
#[test]
fn the_risk_per_trade_cut_is_one_line_with_before_after_and_reason() {
    let s = Scenario::kr();
    let draft = buy_with("100");
    assert_eq!(
        limit_clamps(&draft, &s.ctx()),
        vec![line(LimitKind::RiskPerTrade, "100", "33")]
    );
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("33"));
    assert_eq!(passed.amount(), d("1650000"));
}

// 검증: R4-4 C24 T31 P8 P7
#[test]
fn a_symbol_weight_cut_is_one_line() {
    // 종목 10% → 20주. AI 수량 33주는 1회 위험(33)과 같아 그 한도는 줄이지 않는다 → 종목 비중 한 줄.
    let mut s = Scenario::kr();
    s.params.symbol_weight_pct = d("10");
    let draft = buy_with("33");
    assert_eq!(
        limit_clamps(&draft, &s.ctx()),
        vec![line(LimitKind::SymbolWeight, "33", "20")]
    );
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("20"));
    assert_eq!(passed.amount(), d("1000000"));
}

// 검증: R4-4 C24 T31 P1 P8 P7
#[test]
fn two_limits_cutting_in_turn_leave_two_lines_in_the_applied_order() {
    // AI 100 → 1회 위험 33 → 종목 비중 20(종목 10%). 앞 줄의 뒤(33)가 다음 줄의 앞이다.
    let mut s = Scenario::kr();
    s.params.symbol_weight_pct = d("10");
    let draft = buy_with("100");
    let clamps = limit_clamps(&draft, &s.ctx());
    assert_eq!(
        clamps,
        vec![
            line(LimitKind::RiskPerTrade, "100", "33"),
            line(LimitKind::SymbolWeight, "33", "20"),
        ]
    );
    assert_eq!(clamps[0].after, clamps[1].before);
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("20"));
}

// 검증: R4-4 C24 T31 P1 P8 P7
#[test]
fn three_limits_in_turn_end_with_the_sector_weight() {
    // 하이닉스 70주(3,500,000원)를 이미 들고 있어 같은 업종의 남은 칸은 4,000,000 − 3,500,000 → 10주.
    // AI 100 → 위험 33 → 종목 20(종목 10%) → 업종 10.
    let mut s = Scenario::kr();
    s.params.symbol_weight_pct = d("10");
    s.hold(HYNIX, "70", "50000", "50000");
    let draft = buy_with("100");
    let clamps = limit_clamps(&draft, &s.ctx());
    assert_eq!(
        clamps,
        vec![
            line(LimitKind::RiskPerTrade, "100", "33"),
            line(LimitKind::SymbolWeight, "33", "20"),
            line(LimitKind::SectorWeight, "20", "10"),
        ]
    );
    for pair in clamps.windows(2) {
        assert_eq!(pair[0].after, pair[1].before);
    }
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("10"));
    assert_eq!(passed.amount(), d("500000"));
}

// 검증: R4-4 C24 T31 P7
#[test]
fn without_an_ai_quantity_there_is_no_before_value_so_nothing_is_recorded() {
    // AI 수량이 없으면 줄이기 전 값이 없다(보수적으로 기록하지 않는다). 수량은 한도가 그대로 정한다.
    let s = Scenario::kr();
    assert_eq!(limit_clamps(&kr_buy(), &s.ctx()), Vec::new());
    let passed = expect_pass(s.check(&kr_buy()));
    assert_eq!(passed.order().quantity, d("33"));
    // 같은 장면에서 AI 수량을 주면 같은 수량(33)에 줄임 기록이 붙는다 — 기록은 판정을 바꾸지 않는다.
    let with_ai = expect_pass(s.check(&buy_with("100")));
    assert_eq!(with_ai.order(), passed.order());
    assert_eq!(limit_clamps(&buy_with("100"), &s.ctx()).len(), 1);
}

// 검증: R4-4 C24 T31 E21 V25
#[test]
fn a_buy_waiting_for_approval_records_only_the_sizing_cuts() {
    // 자동 주문 한도 1,000,000원 < 1,650,000원 → 승인 대기. 승인 대기는 수량을 줄이지 않으므로
    // 자동 주문 한도 줄이기는 기록하지 않는다(답이 없을 때 줄이는 길이 따로 있다).
    let s = limited_scenario();
    let draft = buy_with("100");
    let pending = pending_for(&s, &draft);
    assert_eq!(pending.order().quantity, d("33"));
    assert_eq!(
        limit_clamps(&draft, &s.ctx()),
        vec![line(LimitKind::RiskPerTrade, "100", "33")]
    );
}

// 검증: R4-4 C24 T31 E11 E31
#[test]
fn a_blocked_check_or_a_sell_records_nothing() {
    let mut s = Scenario::kr();
    s.team = TeamStatus::DailyLimitHalt;
    assert_eq!(limit_clamps(&buy_with("100"), &s.ctx()), Vec::new());

    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "45000", "50000");
    let sell = OrderDraft {
        ai_quantity: Some(d("5")),
        ..kr_sell(Purpose::StopLoss)
    };
    assert_eq!(limit_clamps(&sell, &s.ctx()), Vec::new());
    // 한도에 걸린 사기를 같은 장면에서 대조한다(기록이 비어 있는 것이 우연이 아님).
    let s = Scenario::kr();
    assert!(!limit_clamps(&buy_with("100"), &s.ctx()).is_empty());
}

// 검증: R4-4 C24 T31 E22 E21 P12
#[test]
fn an_unanswered_buy_cut_by_the_auto_order_limit_records_one_line() {
    // 기다리던 33주 → 한도(수수료 넣어) 안 19주.
    let s = limited_scenario();
    let draft = kr_buy();
    let (passed, clamps) =
        reduce_within_limit_recorded(pending_for(&s, &draft), &draft, &s.ctx()).unwrap();
    assert_eq!(clamps, vec![line(LimitKind::AutoOrderLimit, "33", "19")]);
    assert_eq!(passed.order().quantity, d("19"));
}

// 검증: R4-4 C24 T31 E22 E21 P12
#[test]
fn the_recorded_reduce_gives_the_same_order_as_the_plain_one() {
    let s = limited_scenario();
    let draft = kr_buy();
    let plain = reduce_within_limit(pending_for(&s, &draft), &draft, &s.ctx()).unwrap();
    let (recorded, clamps) =
        reduce_within_limit_recorded(pending_for(&s, &draft), &draft, &s.ctx()).unwrap();
    assert_eq!(clamps.len(), 1);
    assert_eq!(recorded.order(), plain.order());
    assert_eq!(recorded.amount(), plain.amount());
    assert!(recorded.approval().is_none());
    assert_eq!(
        recorded.order(),
        &approved(Side::Buy, OrderType::Limit, "19", Some("50000"))
    );
}

// 검증: R4-4 C24 T31 E22 P1 P7
#[test]
fn the_auto_limit_line_follows_the_sizing_lines_of_the_same_buy() {
    // AI 100 → 위험 33(한 줄) → 자동 주문 한도 19(이어지는 줄). 앞 줄의 뒤가 다음 줄의 앞이다.
    let s = limited_scenario();
    let draft = buy_with("100");
    let (passed, clamps) =
        reduce_within_limit_recorded(pending_for(&s, &draft), &draft, &s.ctx()).unwrap();
    assert_eq!(
        clamps,
        vec![
            line(LimitKind::RiskPerTrade, "100", "33"),
            line(LimitKind::AutoOrderLimit, "33", "19"),
        ]
    );
    assert_eq!(clamps[0].after, clamps[1].before);
    assert_eq!(passed.order().quantity, d("19"));
}

// 검증: R4-4 C24 T31 E22 P12 V24
#[test]
fn the_recorded_reduce_blocks_exactly_like_the_plain_one() {
    // 한도 0.4% = 40,000원 < 50,000원 → 한 주도 못 산다. 막힘은 같고 기록은 나오지 않는다.
    let draft = kr_buy();
    let mut s = limited_scenario();
    let pending = pending_for(&s, &draft);
    let pending_again = pending_for(&s, &draft);
    s.params.auto_order_limit_pct = d("0.4");
    let plain = reduce_within_limit(pending, &draft, &s.ctx()).unwrap_err();
    let recorded = reduce_within_limit_recorded(pending_again, &draft, &s.ctx()).unwrap_err();
    assert_eq!(recorded.kind, plain.kind);
    assert_eq!(recorded.reason, plain.reason);
}

// 검증: R4-4 C24 T31 E21 V25 P12
#[test]
fn recording_does_not_change_the_check_verdict() {
    // 같은 장면의 판정은 기록을 부르기 전과 뒤가 같다(순수한 읽기).
    let s = Scenario::kr();
    let draft = buy_with("100");
    let before = s.check(&draft);
    assert_eq!(limit_clamps(&draft, &s.ctx()).len(), 1);
    let after = check(&draft, &s.ctx());
    assert_eq!(before, after);
}
