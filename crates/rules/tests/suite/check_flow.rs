//! 주문 전 점검의 흐름(C9 · FN1): 판단 상태 · 팀 상태 · 금액 한도 · 수량과 값 · 점검 통과 표시.

use crate::common::*;
use pi_market::Market;
use pi_paper::{InitialCash, OrderType, PaperBroker, PaperConfig, Side};
use pi_rules::{
    BlockKind, Outcome, Purpose, RationaleField, Reason, TeamStatus, TradeStage, issue,
};

// 검증: C9 P1 P7 P8 P13
#[test]
fn a_complete_buy_passes_with_the_quantity_and_price_the_engine_computed() {
    // 위험 금액 10,000,000 × 1% = 100,000, 주당 위험 50,000 − 47,000 = 3,000 → floor(33.33) = 33.
    // 종목 상한 2,000,000 / 50,000 = 40, 업종 상한 4,000,000 / 50,000 = 80 → 33.
    let s = Scenario::kr();
    let passed = expect_pass(s.check(&kr_buy()));
    assert_eq!(
        passed.order(),
        &approved(
            Market::Kr,
            SAMSUNG,
            Side::Buy,
            OrderType::Limit,
            "33",
            Some("50000")
        )
    );
    assert_eq!(passed.amount(), d("1650000"));
}

// 검증: C9 P1 P8
#[test]
fn a_us_buy_is_sized_in_dollars_only() {
    // 위험 금액 10,000.00 × 1% = 100, 주당 위험 1.50 → floor(66.67) = 66. 종목 상한 2,000 / 25.5 = 78.
    let s = Scenario::us();
    let passed = expect_pass(s.check(&us_buy()));
    assert_eq!(
        passed.order(),
        &approved(
            Market::Us,
            APPLE,
            Side::Buy,
            OrderType::Limit,
            "66",
            Some("25.50")
        )
    );
    assert_eq!(passed.amount(), d("1683.00"));
}

// 검증: E20
#[test]
fn every_stage_other_than_judged_is_blocked() {
    let s = Scenario::kr();
    for stage in TradeStage::ALL {
        if stage == TradeStage::Judged {
            continue;
        }
        let draft = pi_rules::OrderDraft { stage, ..kr_buy() };
        let blocked = expect_blocked(s.check(&draft));
        assert_eq!(blocked.reason, Reason::NotJudged(stage), "{stage:?}");
        assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
        assert!(!blocked.detail().trim().is_empty());
    }
}

// 검증: E20 E31 E10
#[test]
fn the_stage_is_looked_at_before_the_team_and_the_team_before_the_rationale() {
    let mut s = Scenario::kr();
    s.team = TeamStatus::FullStop;
    let mut draft = kr_buy();
    draft.rationale.hypothesis = None;

    draft.stage = TradeStage::Discussing;
    let blocked = expect_blocked(s.check(&draft));
    assert_eq!(blocked.reason, Reason::NotJudged(TradeStage::Discussing));

    draft.stage = TradeStage::Judged;
    let blocked = expect_blocked(s.check(&draft));
    assert_eq!(
        blocked.reason,
        Reason::TeamNotOperating(TeamStatus::FullStop)
    );

    s.team = TeamStatus::Operating;
    let blocked = expect_blocked(s.check(&draft));
    assert_eq!(
        blocked.reason,
        Reason::MissingRationale(vec![RationaleField::Hypothesis])
    );
}

// 검증: E31 P3 P4 P11
#[test]
fn buys_are_blocked_in_every_state_but_operating() {
    for team in TeamStatus::ALL {
        if team == TeamStatus::Operating {
            continue;
        }
        for purpose in [Purpose::NewEntry, Purpose::AddOn] {
            let mut s = Scenario::kr();
            s.team = team;
            s.hold(SAMSUNG, "10", "45000", "50000");
            let draft = pi_rules::OrderDraft {
                purpose,
                ..kr_buy()
            };
            let blocked = expect_blocked(s.check(&draft));
            assert_eq!(
                blocked.reason,
                Reason::TeamNotOperating(team),
                "{team:?} {purpose:?}"
            );
            assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
            assert!(!blocked.detail().trim().is_empty());
        }
    }
}

// 검증: E34 P3 P4 P11
#[test]
fn stop_loss_and_liquidation_pass_in_every_team_state() {
    for team in TeamStatus::ALL {
        for purpose in [Purpose::StopLoss, Purpose::Liquidation] {
            let mut s = Scenario::kr();
            s.team = team;
            s.hold(SAMSUNG, "10", "55000", "50000");
            let draft = pi_rules::OrderDraft {
                purpose,
                ..kr_stop_loss()
            };
            let passed = expect_pass(s.check(&draft));
            assert_eq!(
                passed.order(),
                &approved(
                    Market::Kr,
                    SAMSUNG,
                    Side::Sell,
                    OrderType::Market,
                    "10",
                    None
                ),
                "{team:?} {purpose:?}"
            );
            // 시장가 팔기의 금액은 시세로 센다: 10 × 50,000.
            assert_eq!(passed.amount(), d("500000"));
        }
    }
}

// 검증: P11 N6 E34
#[test]
fn stop_loss_still_passes_when_the_quote_is_late_or_the_account_is_off() {
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "55000", "50000");
    s.quote_age(31);
    s.health.account_in_sync = false;
    s.health.api_error = true;
    s.health.data_anomaly = true;
    let passed = expect_pass(s.check(&kr_stop_loss()));
    assert_eq!(passed.order().side, Side::Sell);
    assert_eq!(passed.order().quantity, d("10"));
}

// 검증: P3 E23 E34
#[test]
fn stop_loss_passes_after_the_daily_loss_limit_is_reached() {
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "55000", "50000");
    s.book.equity = d("9700000");
    let passed = expect_pass(s.check(&kr_stop_loss()));
    assert_eq!(passed.order().quantity, d("10"));
}

// 검증: P7 P5
#[test]
fn a_sell_never_exceeds_what_is_held() {
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "45000", "50000");
    let draft = pi_rules::OrderDraft {
        purpose: Purpose::TakeProfit,
        order_type: OrderType::Limit,
        limit_price: Some(d("56000")),
        ai_quantity: Some(d("20")),
        ..kr_buy()
    };
    let passed = expect_pass(s.check(&draft));
    assert_eq!(
        passed.order(),
        &approved(
            Market::Kr,
            SAMSUNG,
            Side::Sell,
            OrderType::Limit,
            "10",
            Some("56000")
        )
    );
}

// 검증: P7
#[test]
fn the_smaller_of_the_ai_quantity_and_the_engine_quantity_wins() {
    let s = Scenario::kr();
    for (ai, want) in [("10", "10"), ("50", "33"), ("10.7", "10"), ("33", "33")] {
        let draft = pi_rules::OrderDraft {
            ai_quantity: Some(d(ai)),
            ..kr_buy()
        };
        let passed = expect_pass(s.check(&draft));
        assert_eq!(passed.order().quantity, d(want), "AI 수량 {ai}");
    }
}

// 검증: P7
#[test]
fn the_buy_price_is_the_smaller_of_the_ai_price_and_the_quote_on_a_tick() {
    let s = Scenario::kr();
    // (AI 값, 주문 값, 수량). 20,000 이상 50,000 미만의 호가 단위는 50원.
    // 48,000: 주당 위험 1,000 → 100, 종목 상한 2,000,000 / 48,000 → 41.
    let cases = [
        ("50020", "50000", "33"),
        ("49975", "49950", "33"),
        ("48000", "48000", "41"),
    ];
    for (ai, price, quantity) in cases {
        let draft = pi_rules::OrderDraft {
            limit_price: Some(d(ai)),
            ..kr_buy()
        };
        let passed = expect_pass(s.check(&draft));
        assert_eq!(passed.order().price, Some(d(price)), "AI 값 {ai}");
        assert_eq!(passed.order().quantity, d(quantity), "AI 값 {ai}");
    }
}

// 검증: E21 V25
#[test]
fn a_buy_over_the_auto_order_limit_waits_for_approval() {
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("10");
    match s.check(&kr_buy()) {
        Outcome::NeedsApproval(pending) => {
            assert_eq!(
                pending.order(),
                &approved(
                    Market::Kr,
                    SAMSUNG,
                    Side::Buy,
                    OrderType::Limit,
                    "33",
                    Some("50000")
                )
            );
            assert_eq!(pending.amount(), d("1650000"));
            assert_eq!(pending.limit(), d("1000000"));
        }
        other => panic!("승인을 기다려야 한다: {other:?}"),
    }
}

// 검증: E21 V25
#[test]
fn a_buy_exactly_at_the_auto_order_limit_passes() {
    // 10,000,000 × 16.5% = 1,650,000 = 수량 × 값. 5차(결정 14)부터 한도와 비교하는 금액은 수수료를
    // 넣은 1,650,000 + 올림(247.5) = 1,650,248이라 한도를 넘는다 → 승인대기(전에는 통과).
    // 보이는 금액은 수량 × 값 그대로다. (함수 이름은 잠금 판 그대로 둔다.)
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("16.5");
    match s.check(&kr_buy()) {
        Outcome::NeedsApproval(pending) => {
            assert_eq!(pending.amount(), d("1650000"));
            assert_eq!(pending.limit(), d("1650000"));
        }
        other => panic!("수수료를 넣으면 한도를 넘어 승인을 기다려야 한다: {other:?}"),
    }
}

// 검증: E21 E34
#[test]
fn a_stop_loss_over_the_auto_order_limit_does_not_wait_for_approval() {
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("1");
    s.hold(SAMSUNG, "10", "55000", "50000");
    let passed = expect_pass(s.check(&kr_stop_loss()));
    assert_eq!(passed.amount(), d("500000"));
}

// 검증: P1 P8
#[test]
fn template_values_over_the_absolute_caps_are_capped_again() {
    // R 5% → 2%: 200,000 / 3,000 = 66. 종목 100% → 25%: 2,500,000 / 50,000 = 50. 업종 100% → 50%: 100.
    let mut s = Scenario::kr();
    s.params.risk_per_trade_pct = d("5");
    s.params.symbol_weight_pct = d("100");
    s.params.sector_weight_pct = d("100");
    s.params.auto_order_limit_pct = d("30");
    let passed = expect_pass(s.check(&kr_buy()));
    assert_eq!(passed.order().quantity, d("50"));
    assert_eq!(passed.amount(), d("2500000"));
}

// 검증: P3
#[test]
fn a_daily_loss_limit_over_the_cap_is_capped_again() {
    // 하루 손실 한도 8% → 상한 5%. 그날 손실 5%면 막힌다.
    let mut s = Scenario::kr();
    s.params.daily_loss_limit_pct = d("8");
    s.book.equity = d("9500000");
    let blocked = expect_blocked(s.check(&kr_buy()));
    assert!(
        blocked
            .reason
            .principles()
            .contains(&pi_rules::Principle::P3)
    );
}

// 검증: P4
#[test]
fn a_drawdown_limit_over_the_cap_is_capped_again() {
    // 최대 낙폭 한도 40% → 상한 20%. 12,500,000 → 10,000,000은 20%.
    let mut s = Scenario::kr();
    s.params.max_drawdown_pct = d("40");
    s.book.peak_equity = d("12500000");
    let blocked = expect_blocked(s.check(&kr_buy()));
    assert!(
        blocked
            .reason
            .principles()
            .contains(&pi_rules::Principle::P4)
    );
}

// 검증: P10
#[test]
fn a_source_known_exactly_at_the_decision_time_is_fine() {
    let s = Scenario::kr();
    let mut draft = kr_buy();
    draft.sources[0].known_at = draft.decided_at;
    expect_pass(s.check(&draft));
}

// 검증: N6 P11
#[test]
fn a_quote_exactly_thirty_seconds_old_is_not_late() {
    let mut s = Scenario::kr();
    s.quote_age(30);
    expect_pass(s.check(&kr_buy()));
}

// 검증: P6
#[test]
fn adding_to_a_winning_position_passes() {
    // 가진 것 10주 × 50,000 = 500,000(5%). 종목 상한 남은 1,500,000 / 50,000 = 30 → min(33, 30) = 30.
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "45000", "50000");
    let draft = pi_rules::OrderDraft {
        purpose: Purpose::AddOn,
        ..kr_buy()
    };
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("30"));
}

// 검증: P13
#[test]
fn a_buy_with_a_positive_edge_after_costs_passes() {
    // 비용 = 50,000 × (2 × 0.00015 + 0.0020 + 2 × 0.001) = 215. 기대값 216 → 1 > 0.
    let s = Scenario::kr();
    let mut draft = kr_buy();
    draft.rationale.expected_value = Some(d("216"));
    expect_pass(s.check(&draft));
}

// 검증: E33
#[test]
fn issue_turns_a_pass_into_a_permit_for_the_same_order() {
    let config = PaperConfig::new(
        InitialCash {
            krw: d("10000000"),
            usd: d("0"),
        },
        kr_rules(),
        us_rules(),
    );
    let (_broker, authority) = PaperBroker::open(config).unwrap();
    let s = Scenario::kr();
    let passed = expect_pass(s.check(&kr_buy()));
    let want = passed.order().clone();
    let permit = issue(&authority, passed);
    assert_eq!(permit.approved(), &want);
}
