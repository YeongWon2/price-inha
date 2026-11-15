//! 2차 1 · 3번: 아래쪽 경계 · 범위 모양 · 업종 대 종목 비중을 거부한다(C14 · E5 · V28 · M3 · N2 · N4).

use std::time::Duration;

use pi_template::{
    ApprovalWait, Change, Cooldown, CountRange, Error, Invalid, InvalidReason, LossStreakStop,
    MaxHolding, OverCap, ParamField, PctRange, Rebalance, RiskParams, Style, Template,
    UnfilledCancel, adjust, preset,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

/// 거부되어야 하는 조정. 지금 값이 그대로인지도 본다.
fn rejected(current: &RiskParams, changes: &[Change]) -> OverCap {
    let before = current.clone();
    let err = match adjust(current, changes) {
        Ok(next) => panic!("거부되어야 함: {changes:?} → {next:?}"),
        Err(e) => e,
    };
    assert_eq!(current, &before, "거부된 조정이 지금 값을 바꿈");
    err
}

fn has(err: &OverCap, field: ParamField, reason: InvalidReason) -> bool {
    err.invalid.contains(&Invalid { field, reason })
}

/// 고친 JSON 을 읽으면 거부되어 그 칸 · 이유가 담겨야 한다.
fn json_rejected(edit: impl FnOnce(&mut Value), field: ParamField, reason: InvalidReason) {
    let text = Template::new(Style::Swing).to_json();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    edit(&mut v);
    match Template::from_json(&v.to_string()) {
        Err(Error::OverCap(over)) => {
            assert!(has(&over, field, reason), "{field:?} {reason:?}: {over:?}");
        }
        other => panic!("{field:?} {reason:?} 가 거부되지 않음: {other:?}"),
    }
}

// 검증: C14, E5
#[test]
fn zero_or_negative_capped_ratios_are_rejected() {
    let current = preset(Style::Swing);
    for bad in ["0", "-1", "-0.0001"] {
        let cases = [
            (Change::RiskPerTradePct(d(bad)), ParamField::RiskPerTrade),
            (
                Change::DailyLossLimitPct(d(bad)),
                ParamField::DailyLossLimit,
            ),
            (Change::MaxDrawdownPct(d(bad)), ParamField::MaxDrawdown),
            (Change::SymbolWeightPct(d(bad)), ParamField::SymbolWeight),
            (
                Change::ConvictionWeightPct(Some(d(bad))),
                ParamField::ConvictionWeight,
            ),
            (Change::SectorWeightPct(d(bad)), ParamField::SectorWeight),
        ];
        for (change, field) in cases {
            let err = rejected(&current, std::slice::from_ref(&change));
            assert!(
                has(&err, field, InvalidReason::NotPositive),
                "{change:?}: {err:?}"
            );
            assert!(err.violations.is_empty(), "상한 위반이 아님: {err:?}");
        }
    }
}

// 검증: C14, E5
#[test]
fn smallest_positive_ratios_are_accepted() {
    let current = preset(Style::Swing);
    let next = adjust(
        &current,
        &[
            Change::RiskPerTradePct(d("0.0001")),
            Change::DailyLossLimitPct(d("0.0001")),
            Change::MaxDrawdownPct(d("0.0001")),
        ],
    )
    .unwrap();
    assert_eq!(next.risk_per_trade_pct, d("0.0001"));
    assert_eq!(next.daily_loss_limit_pct, d("0.0001"));
    assert_eq!(next.max_drawdown_pct, d("0.0001"));
}

// 검증: C14, E5
#[test]
fn ratios_over_one_hundred_percent_are_rejected() {
    let current = preset(Style::Swing);
    let over = d("100.0001");
    let cases = [
        (Change::MinCashPct(over), ParamField::MinCash),
        (Change::AutoOrderLimitPct(over), ParamField::AutoOrderLimit),
        (Change::ChaseLimitPct(Some(over)), ParamField::ChaseLimit),
        (
            Change::Stop(pi_template::StopRule {
                fixed_loss_pct: Some(PctRange {
                    low: d("7"),
                    high: over,
                }),
                ..current.stop.clone()
            }),
            ParamField::Stop,
        ),
        (
            Change::Rebalance(Some(Rebalance {
                every_trading_days: 60,
                drift_pct: over,
            })),
            ParamField::Rebalance,
        ),
    ];
    for (change, field) in cases {
        let err = rejected(&current, std::slice::from_ref(&change));
        assert!(
            has(&err, field, InvalidReason::Over100Pct),
            "{change:?}: {err:?}"
        );
    }
}

// 검증: C14, E5
#[test]
fn exactly_one_hundred_percent_is_accepted() {
    let current = preset(Style::Swing);
    let next = adjust(
        &current,
        &[
            Change::MinCashPct(d("100")),
            Change::AutoOrderLimitPct(d("100")),
        ],
    )
    .unwrap();
    assert_eq!(next.min_cash_pct, d("100"));
    assert_eq!(next.auto_order_limit_pct, d("100"));
}

// 검증: C14, E5
#[test]
fn reversed_ranges_are_rejected() {
    let current = preset(Style::Swing);
    let err = rejected(
        &current,
        &[Change::MaxPositions(CountRange { low: 9, high: 8 })],
    );
    assert!(has(
        &err,
        ParamField::MaxPositions,
        InvalidReason::RangeReversed
    ));

    let reversed_stop = pi_template::StopRule {
        fixed_loss_pct: Some(PctRange {
            low: d("8"),
            high: d("7.9999"),
        }),
        ..current.stop.clone()
    };
    let err = rejected(&current, &[Change::Stop(reversed_stop)]);
    assert!(has(&err, ParamField::Stop, InvalidReason::RangeReversed));
}

// 검증: C14
#[test]
fn single_value_ranges_are_accepted() {
    let current = preset(Style::Swing);
    let one_stop = pi_template::StopRule {
        fixed_loss_pct: Some(PctRange {
            low: d("7"),
            high: d("7"),
        }),
        ..current.stop.clone()
    };
    let next = adjust(
        &current,
        &[
            Change::MaxPositions(CountRange { low: 6, high: 6 }),
            Change::Stop(one_stop.clone()),
        ],
    )
    .unwrap();
    assert_eq!(next.max_positions, CountRange { low: 6, high: 6 });
    assert_eq!(next.stop, one_stop);
}

// 검증: C14, E5
#[test]
fn zero_counts_are_rejected() {
    let current = preset(Style::Swing);
    let cases = [
        (
            Change::MaxPositions(CountRange { low: 0, high: 5 }),
            ParamField::MaxPositions,
        ),
        (Change::MaxTradesPerDay(0), ParamField::MaxTradesPerDay),
        (
            Change::LossStreak(Some(LossStreakStop {
                losses: 0,
                cooldown: Cooldown::RestOfDay,
            })),
            ParamField::LossStreak,
        ),
        (
            Change::LossStreak(Some(LossStreakStop {
                losses: 3,
                cooldown: Cooldown::TradingDays(0),
            })),
            ParamField::LossStreak,
        ),
        (
            Change::MaxHolding(MaxHolding::TradingDays(0)),
            ParamField::MaxHolding,
        ),
        (
            Change::Rebalance(Some(Rebalance {
                every_trading_days: 0,
                drift_pct: d("5"),
            })),
            ParamField::Rebalance,
        ),
    ];
    for (change, field) in cases {
        let err = rejected(&current, std::slice::from_ref(&change));
        assert!(
            has(&err, field, InvalidReason::NotPositive),
            "{change:?}: {err:?}"
        );
    }
}

// 검증: N2, C14, E5
#[test]
fn zero_approval_wait_is_rejected() {
    let current = preset(Style::DayTrade);
    let base = current.approval_wait;
    let zero = Duration::ZERO;
    for wait in [
        ApprovalWait {
            stop_loss: zero,
            ..base
        },
        ApprovalWait {
            take_profit: zero,
            ..base
        },
        ApprovalWait { buy: zero, ..base },
    ] {
        let err = rejected(&current, &[Change::ApprovalWait(wait)]);
        assert!(
            has(&err, ParamField::ApprovalWait, InvalidReason::NotPositive),
            "{wait:?}: {err:?}"
        );
    }
}

// 검증: N4, C14, E5
#[test]
fn zero_unfilled_cancel_is_rejected_but_at_close_is_fine() {
    let current = preset(Style::DayTrade);
    let err = rejected(
        &current,
        &[Change::UnfilledCancel(UnfilledCancel::After(
            Duration::ZERO,
        ))],
    );
    assert!(has(
        &err,
        ParamField::UnfilledCancel,
        InvalidReason::NotPositive
    ));
    let next = adjust(&current, &[Change::UnfilledCancel(UnfilledCancel::AtClose)]).unwrap();
    assert_eq!(next.unfilled_cancel, UnfilledCancel::AtClose);
}

// 검증: E5, C14
#[test]
fn one_bad_lower_bound_rejects_the_whole_batch() {
    let current = preset(Style::Swing);
    let err = rejected(
        &current,
        &[
            Change::RiskPerTradePct(d("0.8")),
            Change::MaxTradesPerDay(0),
            Change::MinCashPct(d("20")),
        ],
    );
    assert!(err.violations.is_empty());
    assert_eq!(
        err.invalid,
        vec![Invalid {
            field: ParamField::MaxTradesPerDay,
            reason: InvalidReason::NotPositive,
        }]
    );
}

// 검증: E5, V28
#[test]
fn over_cap_and_lower_bound_are_reported_together() {
    let current = preset(Style::Swing);
    let err = rejected(
        &current,
        &[
            Change::RiskPerTradePct(d("3")),
            Change::DailyLossLimitPct(d("-1")),
        ],
    );
    assert_eq!(err.violations.len(), 1);
    assert_eq!(err.violations[0].cap, d("2"));
    assert!(has(
        &err,
        ParamField::DailyLossLimit,
        InvalidReason::NotPositive
    ));
    assert!(!err.to_string().is_empty());
}

// 검증: M3, C14, E5
#[test]
fn sector_weight_below_symbol_weight_is_rejected() {
    // 스윙: 종목 15% · 업종 35%.
    let current = preset(Style::Swing);
    let err = rejected(&current, &[Change::SectorWeightPct(d("14.9999"))]);
    assert!(has(
        &err,
        ParamField::SectorWeight,
        InvalidReason::SectorBelowSymbol
    ));

    let err = rejected(
        &current,
        &[
            Change::SectorWeightPct(d("20")),
            Change::SymbolWeightPct(d("25")),
        ],
    );
    assert!(has(
        &err,
        ParamField::SectorWeight,
        InvalidReason::SectorBelowSymbol
    ));
    assert!(err.violations.is_empty());
}

// 검증: M3, C14
#[test]
fn sector_weight_equal_to_symbol_weight_is_accepted() {
    let current = preset(Style::Swing);
    let next = adjust(&current, &[Change::SectorWeightPct(d("15"))]).unwrap();
    assert_eq!(next.sector_weight_pct, d("15"));

    // 결과로 판단한다: 종목과 업종을 함께 올리면 받는다.
    let next = adjust(
        &current,
        &[
            Change::SymbolWeightPct(d("25")),
            Change::SectorWeightPct(d("25")),
        ],
    )
    .unwrap();
    assert_eq!(next.symbol_weight_pct, d("25"));
    assert_eq!(next.sector_weight_pct, d("25"));
}

// 검증: M3, C14, E5
#[test]
fn sector_weight_below_conviction_weight_is_rejected() {
    // 중장기: 종목 10% · 확신 종목 20% · 업종 40%.
    let current = preset(Style::LongTerm);
    let err = rejected(&current, &[Change::SectorWeightPct(d("19.9999"))]);
    assert!(has(
        &err,
        ParamField::SectorWeight,
        InvalidReason::SectorBelowSymbol
    ));

    let err = rejected(
        &current,
        &[
            Change::SectorWeightPct(d("22")),
            Change::ConvictionWeightPct(Some(d("25"))),
        ],
    );
    assert!(has(
        &err,
        ParamField::SectorWeight,
        InvalidReason::SectorBelowSymbol
    ));

    let next = adjust(&current, &[Change::SectorWeightPct(d("20"))]).unwrap();
    assert_eq!(next.sector_weight_pct, d("20"));

    // 확신 종목 예외를 없애면 종목 비중만 본다.
    let next = adjust(
        &current,
        &[
            Change::ConvictionWeightPct(None),
            Change::SectorWeightPct(d("10")),
        ],
    )
    .unwrap();
    assert_eq!(next.sector_weight_pct, d("10"));
}

// 검증: C4, V27
#[test]
fn every_preset_passes_the_lower_bound_checks() {
    for style in Style::ALL {
        let p = preset(style);
        assert_eq!(adjust(&p, &[]).unwrap(), p, "{style:?}");
        let t = Template::new(style);
        assert_eq!(Template::from_json(&t.to_json()).unwrap(), t, "{style:?}");
    }
}

// 검증: E5, V28
#[test]
fn json_with_zero_or_negative_ratio_is_rejected() {
    json_rejected(
        |v| v["params"]["risk_per_trade_pct"] = Value::from("-1"),
        ParamField::RiskPerTrade,
        InvalidReason::NotPositive,
    );
    json_rejected(
        |v| v["params"]["max_drawdown_pct"] = Value::from("0"),
        ParamField::MaxDrawdown,
        InvalidReason::NotPositive,
    );
    json_rejected(
        |v| v["params"]["sector_weight_pct"] = Value::from("0"),
        ParamField::SectorWeight,
        InvalidReason::NotPositive,
    );
}

// 검증: E5
#[test]
fn json_with_ratio_over_one_hundred_percent_is_rejected() {
    json_rejected(
        |v| v["params"]["min_cash_pct"] = Value::from("100.0001"),
        ParamField::MinCash,
        InvalidReason::Over100Pct,
    );
}

// 검증: E5
#[test]
fn json_with_reversed_range_is_rejected() {
    json_rejected(
        |v| v["params"]["max_positions"] = json!({"low": 9, "high": 8}),
        ParamField::MaxPositions,
        InvalidReason::RangeReversed,
    );
}

// 검증: N2, N4, E5
#[test]
fn json_with_zero_count_or_time_is_rejected() {
    json_rejected(
        |v| v["params"]["max_trades_per_day"] = Value::from(0),
        ParamField::MaxTradesPerDay,
        InvalidReason::NotPositive,
    );
    json_rejected(
        |v| v["params"]["approval_wait"]["buy"] = json!({"secs": 0, "nanos": 0}),
        ParamField::ApprovalWait,
        InvalidReason::NotPositive,
    );
    json_rejected(
        |v| v["params"]["unfilled_cancel"] = json!({"after": {"secs": 0, "nanos": 0}}),
        ParamField::UnfilledCancel,
        InvalidReason::NotPositive,
    );
}

// 검증: M3, E5
#[test]
fn json_with_sector_below_symbol_is_rejected() {
    json_rejected(
        |v| v["params"]["sector_weight_pct"] = Value::from("10"),
        ParamField::SectorWeight,
        InvalidReason::SectorBelowSymbol,
    );
}
