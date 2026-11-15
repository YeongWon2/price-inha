//! 절대 상한과 값 맞추기(C14 · E5 · V28 · M3).

use std::time::Duration;

use pi_template::{
    AbsoluteCaps, ApprovalWait, CappedField, Change, MaxHolding, Style, UnfilledCancel, Violation,
    adjust, preset,
};
use rust_decimal::Decimal;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

// 검증: M3
#[test]
fn absolute_caps_are_the_principle_values() {
    let c = AbsoluteCaps::FIXED;
    assert_eq!(c.risk_per_trade_pct, d("2"));
    assert_eq!(c.daily_loss_pct, d("5"));
    assert_eq!(c.max_drawdown_pct, d("20"));
    assert_eq!(c.symbol_weight_pct, d("25"));
    assert_eq!(c.sector_weight_pct, d("50"));
}

// 검증: C14
#[test]
fn changes_inside_caps_are_applied_and_the_rest_is_kept() {
    let current = preset(Style::Swing);
    let next = adjust(
        &current,
        &[
            Change::RiskPerTradePct(d("1.5")),
            Change::MaxTradesPerDay(2),
            Change::MaxHolding(MaxHolding::TradingDays(10)),
            Change::ChaseLimitPct(None),
        ],
    )
    .unwrap();
    assert_eq!(next.risk_per_trade_pct, d("1.5"));
    assert_eq!(next.max_trades_per_day, 2);
    assert_eq!(next.max_holding, MaxHolding::TradingDays(10));
    assert_eq!(next.chase_limit_pct, None);
    assert_eq!(next.daily_loss_limit_pct, current.daily_loss_limit_pct);
    assert_eq!(next.stop, current.stop);
    assert_eq!(next.promotion, current.promotion);
}

// 검증: C14, N2, N4
#[test]
fn approval_wait_and_unfilled_cancel_are_template_values() {
    let current = preset(Style::DayTrade);
    let wait = ApprovalWait {
        stop_loss: Duration::from_secs(30),
        take_profit: Duration::from_secs(120),
        buy: Duration::from_secs(180),
    };
    let next = adjust(
        &current,
        &[
            Change::ApprovalWait(wait),
            Change::UnfilledCancel(UnfilledCancel::After(Duration::from_secs(90))),
        ],
    )
    .unwrap();
    assert_eq!(next.approval_wait, wait);
    assert_eq!(
        next.unfilled_cancel,
        UnfilledCancel::After(Duration::from_secs(90))
    );
}

// 검증: C14, E5
#[test]
fn exactly_the_cap_is_accepted() {
    let current = preset(Style::DayTrade);
    let next = adjust(
        &current,
        &[
            Change::RiskPerTradePct(d("2")),
            Change::DailyLossLimitPct(d("5")),
            Change::MaxDrawdownPct(d("20")),
            Change::SymbolWeightPct(d("25")),
            Change::ConvictionWeightPct(Some(d("25"))),
            Change::SectorWeightPct(d("50")),
        ],
    )
    .unwrap();
    assert_eq!(next.risk_per_trade_pct, d("2"));
    assert_eq!(next.daily_loss_limit_pct, d("5"));
    assert_eq!(next.max_drawdown_pct, d("20"));
    assert_eq!(next.symbol_weight_pct, d("25"));
    assert_eq!(next.conviction_weight_pct, Some(d("25")));
    assert_eq!(next.sector_weight_pct, d("50"));
}

// 검증: E5, V28
#[test]
fn just_over_the_risk_cap_is_rejected_with_field_and_cap() {
    let current = preset(Style::DayTrade);
    let err = adjust(&current, &[Change::RiskPerTradePct(d("2.0001"))]).unwrap_err();
    assert_eq!(
        err.violations,
        vec![Violation {
            field: CappedField::RiskPerTrade,
            requested: d("2.0001"),
            cap: d("2"),
        }]
    );
}

// 검증: V28, E5, C14
#[test]
fn three_percent_risk_is_not_saved_and_shows_two_percent() {
    let current = preset(Style::Swing);
    let before = current.clone();
    let err = adjust(&current, &[Change::RiskPerTradePct(d("3"))]).unwrap_err();
    assert_eq!(err.violations.len(), 1);
    assert_eq!(err.violations[0].field, CappedField::RiskPerTrade);
    assert_eq!(err.violations[0].cap, d("2"));
    assert_eq!(err.violations[0].requested, d("3"));
    assert!(err.to_string().contains("2%"), "{err}");
    assert_eq!(current, before);
    assert_eq!(current.risk_per_trade_pct, d("1"));
}

// 검증: E5
#[test]
fn every_capped_field_rejects_just_over_its_cap() {
    let cases = [
        (
            Change::DailyLossLimitPct(d("5.0001")),
            CappedField::DailyLossLimit,
            d("5.0001"),
            d("5"),
        ),
        (
            Change::MaxDrawdownPct(d("20.0001")),
            CappedField::MaxDrawdown,
            d("20.0001"),
            d("20"),
        ),
        (
            Change::SymbolWeightPct(d("25.0001")),
            CappedField::SymbolWeight,
            d("25.0001"),
            d("25"),
        ),
        (
            Change::ConvictionWeightPct(Some(d("25.0001"))),
            CappedField::ConvictionWeight,
            d("25.0001"),
            d("25"),
        ),
        (
            Change::SectorWeightPct(d("50.0001")),
            CappedField::SectorWeight,
            d("50.0001"),
            d("50"),
        ),
    ];
    let current = preset(Style::LongTerm);
    for (change, field, requested, cap) in cases {
        let err = adjust(&current, std::slice::from_ref(&change)).unwrap_err();
        assert_eq!(
            err.violations,
            vec![Violation {
                field,
                requested,
                cap
            }],
            "{change:?}"
        );
    }
}

// 검증: E5, C14
#[test]
fn one_over_cap_rejects_the_whole_batch_and_keeps_current() {
    let current = preset(Style::Swing);
    let before = current.clone();
    let result = adjust(
        &current,
        &[
            Change::MaxTradesPerDay(1),
            Change::RiskPerTradePct(d("0.8")),
            Change::DailyLossLimitPct(d("6")),
        ],
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.violations,
        vec![Violation {
            field: CappedField::DailyLossLimit,
            requested: d("6"),
            cap: d("5"),
        }]
    );
    assert_eq!(current, before);
    // 같은 묶음의 정상 칸도 적용되지 않았다: 다시 맞추면 지금 값에서 출발한다.
    let next = adjust(&current, &[]).unwrap();
    assert_eq!(next, before);
    assert_eq!(next.max_trades_per_day, 3);
    assert_eq!(next.risk_per_trade_pct, d("1"));
}

// 검증: E5, V28
#[test]
fn all_violations_are_listed_in_field_order() {
    let current = preset(Style::DayTrade);
    let err = adjust(
        &current,
        &[
            Change::SectorWeightPct(d("60")),
            Change::RiskPerTradePct(d("3")),
            Change::MaxDrawdownPct(d("30")),
        ],
    )
    .unwrap_err();
    assert_eq!(
        err.violations,
        vec![
            Violation {
                field: CappedField::RiskPerTrade,
                requested: d("3"),
                cap: d("2"),
            },
            Violation {
                field: CappedField::MaxDrawdown,
                requested: d("30"),
                cap: d("20"),
            },
            Violation {
                field: CappedField::SectorWeight,
                requested: d("60"),
                cap: d("50"),
            },
        ]
    );
    let text = err.to_string();
    assert!(text.contains("2%"), "{text}");
    assert!(text.contains("20%"), "{text}");
    assert!(text.contains("50%"), "{text}");
}

// 검증: C14
#[test]
fn later_change_to_the_same_field_wins() {
    let current = preset(Style::Swing);
    let next = adjust(
        &current,
        &[
            Change::RiskPerTradePct(d("0.7")),
            Change::RiskPerTradePct(d("1.2")),
        ],
    )
    .unwrap();
    assert_eq!(next.risk_per_trade_pct, d("1.2"));
}

// 검증: E5
#[test]
fn an_over_cap_value_is_rejected_even_if_a_later_change_fixes_it() {
    // "하나라도 상한을 넘으면 전체를 거부"를 보수적으로 읽는다: 넣은 값 하나하나를 본다.
    let current = preset(Style::Swing);
    let err = adjust(
        &current,
        &[
            Change::RiskPerTradePct(d("3")),
            Change::RiskPerTradePct(d("1.2")),
        ],
    )
    .unwrap_err();
    assert_eq!(
        err.violations,
        vec![Violation {
            field: CappedField::RiskPerTrade,
            requested: d("3"),
            cap: d("2"),
        }]
    );
}

// 검증: C14, M3
#[test]
fn adjusting_a_copy_never_changes_the_preset() {
    let current = preset(Style::LongTerm);
    let next = adjust(&current, &[Change::SymbolWeightPct(d("5"))]).unwrap();
    assert_eq!(next.symbol_weight_pct, d("5"));
    assert_eq!(preset(Style::LongTerm).symbol_weight_pct, d("10"));
}
