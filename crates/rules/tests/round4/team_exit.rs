//! 1. 전체 정지 중의 파는 주문(F4 7b2 · E24 · 결정 2026-11-21 12번).

use pi_paper::{OrderType, Side};
use pi_rules::{BlockKind, OrderDraft, Purpose, Rationale, Reason, TeamStatus, Violation};

use crate::common::{
    SAMSUNG, Scenario, approved, d, expect_blocked, expect_pass, kr_sell, violations,
};

// 검증: E24 P4
#[test]
fn team_exit_is_a_sell_purpose() {
    assert_eq!(Purpose::TeamExit.side(), Side::Sell);
    assert!(!Purpose::TeamExit.is_buy());
}

// 검증: E24 E34 P4
#[test]
fn full_stop_lets_only_stop_loss_and_liquidation_through() {
    let table = [
        (Purpose::StopLoss, true),
        (Purpose::Liquidation, true),
        (Purpose::TakeProfit, false),
        (Purpose::TeamExit, false),
    ];
    for (purpose, passes) in table {
        let mut s = Scenario::kr();
        s.team = TeamStatus::FullStop;
        s.hold(SAMSUNG, "10", "45000", "50000");
        let outcome = s.check(&kr_sell(purpose));
        if passes {
            let passed = expect_pass(outcome);
            assert_eq!(
                passed.order(),
                &approved(Side::Sell, OrderType::Market, "10", None),
                "{purpose:?}"
            );
        } else {
            let blocked = expect_blocked(outcome);
            assert_eq!(
                blocked.reason,
                Reason::TeamNotOperating(TeamStatus::FullStop),
                "{purpose:?}"
            );
            assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
            assert!(!blocked.detail().trim().is_empty());
        }
    }
}

// 검증: E24 E34 E23 P3 P11
#[test]
fn team_exit_and_take_profit_still_pass_outside_full_stop() {
    for team in [
        TeamStatus::Operating,
        TeamStatus::Resting,
        TeamStatus::DailyLimitHalt,
        TeamStatus::AnomalyHalt,
    ] {
        for purpose in [Purpose::TeamExit, Purpose::TakeProfit] {
            let mut s = Scenario::kr();
            s.team = team;
            s.hold(SAMSUNG, "10", "45000", "50000");
            let passed = expect_pass(s.check(&kr_sell(purpose)));
            assert_eq!(
                passed.order(),
                &approved(Side::Sell, OrderType::Market, "10", None),
                "{team:?} {purpose:?}"
            );
            // 시장가 팔기의 금액은 시세로 센다: 10 × 50,000.
            assert_eq!(passed.amount(), d("500000"), "{team:?} {purpose:?}");
            assert!(passed.approval().is_none());
        }
    }
}

// 검증: E24 P4 P9 E10
#[test]
fn full_stop_blocks_a_take_profit_before_looking_at_the_rationale() {
    for purpose in [Purpose::TakeProfit, Purpose::TeamExit] {
        let mut s = Scenario::kr();
        s.team = TeamStatus::FullStop;
        s.hold(SAMSUNG, "10", "45000", "50000");
        let draft = OrderDraft {
            rationale: Rationale::default(),
            ..kr_sell(purpose)
        };
        let blocked = expect_blocked(s.check(&draft));
        assert_eq!(
            blocked.reason,
            Reason::TeamNotOperating(TeamStatus::FullStop),
            "{purpose:?}"
        );
    }
}

// 검증: P12 P5 E34 E11
#[test]
fn team_exit_is_not_a_duplicate_of_an_open_take_profit_sell() {
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "100", "50000", "48000");
    s.open_orders
        .push(approved(Side::Sell, OrderType::Limit, "100", Some("56000")));
    let passed = expect_pass(s.check(&kr_sell(Purpose::TeamExit)));
    assert_eq!(
        passed.order(),
        &approved(Side::Sell, OrderType::Market, "100", None)
    );
}

// 검증: P5 P9 E10
#[test]
fn team_exit_never_sells_more_than_is_held_and_needs_a_rationale() {
    // AI가 150주를 내도 가진 10주까지만 판다.
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "45000", "50000");
    let draft = OrderDraft {
        ai_quantity: Some(d("150")),
        ..kr_sell(Purpose::TeamExit)
    };
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("10"));

    // 가진 것이 없으면 공매도라 막는다.
    let empty = Scenario::kr();
    let blocked = expect_blocked(empty.check(&kr_sell(Purpose::TeamExit)));
    assert!(violations(&blocked).contains(&Violation::ShortSale));

    // 근거가 빠지면 막는다(청산과 같다).
    let draft = OrderDraft {
        rationale: Rationale::default(),
        ..kr_sell(Purpose::TeamExit)
    };
    let blocked = expect_blocked(s.check(&draft));
    assert!(matches!(blocked.reason, Reason::MissingRationale(_)));
}
