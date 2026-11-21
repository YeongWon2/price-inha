//! 3. VI · 거래정지가 '모름'일 때 시세 추정 결과로(결정 2026-11-21 5번).

use pi_market::TradingStatus;
use pi_paper::Side;
use pi_rules::{Purpose, SymbolStatus, Violation};

use crate::common::{SAMSUNG, Scenario, expect_blocked, expect_pass, kr_buy, kr_sell, violations};

// 검증: P12
#[test]
fn trading_status_turns_into_the_check_flags() {
    let table = [
        (TradingStatus::Normal, Some(false), Some(false)),
        (TradingStatus::Vi, Some(true), None),
        (TradingStatus::Halted, None, Some(true)),
        (TradingStatus::Unknown, None, None),
    ];
    for (status, vi, halted) in table {
        assert_eq!(
            SymbolStatus::from_trading_status(status),
            SymbolStatus { vi, halted },
            "{status:?}"
        );
    }
}

// 검증: P12 E11 E18
#[test]
fn a_buy_with_the_converted_status_is_blocked_unless_normal() {
    let table = [
        (TradingStatus::Vi, Violation::ViActive),
        (TradingStatus::Halted, Violation::TradingHalted),
        (TradingStatus::Unknown, Violation::StatusUnknown),
    ];
    for (status, want) in table {
        let mut s = Scenario::kr();
        s.status = SymbolStatus::from_trading_status(status);
        let blocked = expect_blocked(s.check(&kr_buy()));
        assert!(
            violations(&blocked).contains(&want),
            "{status:?}: {blocked:?}"
        );
        assert!(blocked.detail().contains("P12"), "{}", blocked.detail());
    }

    let mut s = Scenario::kr();
    s.status = SymbolStatus::from_trading_status(TradingStatus::Normal);
    let passed = expect_pass(s.check(&kr_buy()));
    assert_eq!(passed.order().side, Side::Buy);
}

// 검증: P12 E34
#[test]
fn sells_are_not_blocked_by_the_converted_status() {
    for status in [
        TradingStatus::Normal,
        TradingStatus::Vi,
        TradingStatus::Halted,
        TradingStatus::Unknown,
    ] {
        for purpose in [Purpose::StopLoss, Purpose::Liquidation, Purpose::TakeProfit] {
            let mut s = Scenario::kr();
            s.status = SymbolStatus::from_trading_status(status);
            s.hold(SAMSUNG, "10", "45000", "50000");
            let passed = expect_pass(s.check(&kr_sell(purpose)));
            assert_eq!(passed.order().side, Side::Sell, "{status:?} {purpose:?}");
        }
    }
}
