//! 승격 기준 판정(E26 · P14 · M3).

use pi_template::{Criterion, PromotionCriteria, PromotionStats, Verdict};
use rust_decimal::Decimal;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

fn criteria() -> PromotionCriteria {
    PromotionCriteria {
        min_trades: 30,
        min_trading_days: 20,
        net_pnl_above: Decimal::ZERO,
        max_drawdown_pct: d("5"),
        out_of_sample_pnl_above: Some(Decimal::ZERO),
    }
}

fn passing() -> PromotionStats {
    PromotionStats {
        trades: 45,
        trading_days: 25,
        net_pnl: d("125000"),
        max_drawdown_pct: d("3.2"),
        out_of_sample_pnl: Some(d("8000")),
    }
}

// 검증: P14, M3
#[test]
fn result_meeting_every_criterion_passes() {
    assert_eq!(criteria().evaluate(&passing()), Verdict::Passed);
}

// 검증: P14, E26
#[test]
fn exact_boundaries_pass() {
    let s = PromotionStats {
        trades: 30,
        trading_days: 20,
        net_pnl: d("0.0001"),
        max_drawdown_pct: d("5"),
        out_of_sample_pnl: Some(d("0.0001")),
    };
    assert_eq!(criteria().evaluate(&s), Verdict::Passed);
}

// 검증: E26, P14
#[test]
fn too_few_trades_is_short_on_trades() {
    let s = PromotionStats {
        trades: 29,
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&s),
        Verdict::Short(vec![Criterion::Trades])
    );
}

// 검증: E26, P14
#[test]
fn too_few_days_is_short_on_period() {
    let s = PromotionStats {
        trading_days: 19,
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&s),
        Verdict::Short(vec![Criterion::TradingDays])
    );
}

// 검증: E26, P14
#[test]
fn zero_net_pnl_after_costs_is_short() {
    let s = PromotionStats {
        net_pnl: Decimal::ZERO,
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&s),
        Verdict::Short(vec![Criterion::NetPnl])
    );
    let loss = PromotionStats {
        net_pnl: d("-1"),
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&loss),
        Verdict::Short(vec![Criterion::NetPnl])
    );
}

// 검증: E26, P14
#[test]
fn drawdown_just_over_limit_is_short() {
    let s = PromotionStats {
        max_drawdown_pct: d("5.0001"),
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&s),
        Verdict::Short(vec![Criterion::MaxDrawdown])
    );
}

// 검증: E26, P14
#[test]
fn out_of_sample_missing_or_not_positive_is_short() {
    let missing = PromotionStats {
        out_of_sample_pnl: None,
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&missing),
        Verdict::Short(vec![Criterion::OutOfSample])
    );
    let flat = PromotionStats {
        out_of_sample_pnl: Some(Decimal::ZERO),
        ..passing()
    };
    assert_eq!(
        criteria().evaluate(&flat),
        Verdict::Short(vec![Criterion::OutOfSample])
    );
}

// 검증: P14
#[test]
fn out_of_sample_not_required_when_criterion_is_off() {
    let c = PromotionCriteria {
        out_of_sample_pnl_above: None,
        ..criteria()
    };
    let s = PromotionStats {
        out_of_sample_pnl: None,
        ..passing()
    };
    assert_eq!(c.evaluate(&s), Verdict::Passed);
}

// 검증: E26
#[test]
fn every_missing_criterion_is_listed_in_order() {
    let s = PromotionStats {
        trades: 3,
        trading_days: 2,
        net_pnl: d("-500"),
        max_drawdown_pct: d("12"),
        out_of_sample_pnl: None,
    };
    assert_eq!(
        criteria().evaluate(&s),
        Verdict::Short(vec![
            Criterion::Trades,
            Criterion::TradingDays,
            Criterion::NetPnl,
            Criterion::MaxDrawdown,
            Criterion::OutOfSample,
        ])
    );
}

// 검증: P14, M3
#[test]
fn custom_criteria_are_used_as_given() {
    let c = PromotionCriteria {
        min_trades: 50,
        min_trading_days: 40,
        net_pnl_above: d("100000"),
        max_drawdown_pct: d("3"),
        out_of_sample_pnl_above: Some(d("10000")),
    };
    assert_eq!(
        c.evaluate(&passing()),
        Verdict::Short(vec![
            Criterion::Trades,
            Criterion::TradingDays,
            Criterion::MaxDrawdown,
            Criterion::OutOfSample,
        ])
    );
}
