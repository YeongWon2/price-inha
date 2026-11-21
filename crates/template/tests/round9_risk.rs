//! 9차(2026-11-21 발표 자료 대조): 위험을 키우는 변경의 판정 · 묶음 승인 대기(P15 · E5 · E28 · C14 · V28).
//! 지시서 docs/dev/modules/template.md "9차 작업", 결정 docs/dev/decisions.md 2026-11-21 9번 · 13번.

use std::time::Duration;

use pi_template::{
    AdjustPlan, ApprovalWait, AtrBar, AtrStop, CappedField, Change, Cooldown, CountRange,
    EntrySignal, LossStreakStop, Market, MaxHolding, PctRange, Rebalance, RiskField, RiskParams,
    Style, TakeProfitRule, TradingWindow, TriggerKind, UnfilledCancel, adjust, increases_risk,
    plan_adjust, preset, widened_fields,
};
use proptest::prelude::*;
use rust_decimal::Decimal;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

fn swing() -> RiskParams {
    preset(Style::Swing)
}

fn with(base: &RiskParams, f: impl FnOnce(&mut RiskParams)) -> RiskParams {
    let mut p = base.clone();
    f(&mut p);
    p
}

/// 한 줄: `before` → `after` 에서 키운 칸이 `want` 여야 한다.
struct Case {
    name: &'static str,
    before: RiskParams,
    after: RiskParams,
    want: Vec<RiskField>,
}

fn case(
    name: &'static str,
    before: &RiskParams,
    f: impl FnOnce(&mut RiskParams),
    want: &[RiskField],
) -> Case {
    Case {
        name,
        before: before.clone(),
        after: with(before, f),
        want: want.to_vec(),
    }
}

fn check(cases: Vec<Case>) {
    for c in cases {
        assert_eq!(
            widened_fields(&c.before, &c.after),
            c.want,
            "widened_fields: {}",
            c.name
        );
        assert_eq!(
            increases_risk(&c.before, &c.after),
            !c.want.is_empty(),
            "increases_risk: {}",
            c.name
        );
    }
}

/// 지금 값의 칸 전부를 같은 값으로 다시 넣는 변경 묶음.
fn same_changes(p: &RiskParams) -> Vec<Change> {
    vec![
        Change::RiskPerTradePct(p.risk_per_trade_pct),
        Change::DailyLossLimitPct(p.daily_loss_limit_pct),
        Change::MaxDrawdownPct(p.max_drawdown_pct),
        Change::LossStreak(p.loss_streak),
        Change::MaxPositions(p.max_positions),
        Change::SymbolWeightPct(p.symbol_weight_pct),
        Change::ConvictionWeightPct(p.conviction_weight_pct),
        Change::SectorWeightPct(p.sector_weight_pct),
        Change::MinCashPct(p.min_cash_pct),
        Change::Stop(p.stop.clone()),
        Change::TakeProfit(p.take_profit.clone()),
        Change::MaxHolding(p.max_holding),
        Change::EntrySignals(p.entry_signals.clone()),
        Change::Universe(p.universe.clone()),
        Change::AutoOrderLimitPct(p.auto_order_limit_pct),
        Change::TradingWindow(p.trading_window),
        Change::ChaseLimitPct(p.chase_limit_pct),
        Change::MaxTradesPerDay(p.max_trades_per_day),
        Change::Triggers(p.triggers.clone()),
        Change::Rebalance(p.rebalance),
        Change::ApprovalWait(p.approval_wait),
        Change::UnfilledCancel(p.unfilled_cancel),
        Change::Promotion(p.promotion.clone()),
    ]
}

// ── 표의 칸마다: 키우는 변경은 그 칸 하나만, 줄이는 변경은 빈 목록 ──

// 검증: P15, E28, C14
#[test]
fn caps_and_weights_widen_only_when_raised() {
    let s = swing();
    // 종목 비중을 올릴 때 확신 비중(없으면 종목 비중으로 셈)이 함께 오르지 않게 확신 비중을 정해 둔다.
    let s_conv = with(&s, |p| p.conviction_weight_pct = Some(d("25")));
    check(vec![
        case(
            "R% 오름",
            &s,
            |p| p.risk_per_trade_pct = d("1.5"),
            &[RiskField::RiskPerTrade],
        ),
        case("R% 내림", &s, |p| p.risk_per_trade_pct = d("0.5"), &[]),
        case(
            "일일 손실 오름",
            &s,
            |p| p.daily_loss_limit_pct = d("4"),
            &[RiskField::DailyLossLimit],
        ),
        case(
            "일일 손실 내림",
            &s,
            |p| p.daily_loss_limit_pct = d("2"),
            &[],
        ),
        case(
            "최대 낙폭 오름",
            &s,
            |p| p.max_drawdown_pct = d("18"),
            &[RiskField::MaxDrawdown],
        ),
        case("최대 낙폭 내림", &s, |p| p.max_drawdown_pct = d("10"), &[]),
        case(
            "종목 비중 오름",
            &s_conv,
            |p| p.symbol_weight_pct = d("20"),
            &[RiskField::SymbolWeight],
        ),
        case(
            "종목 비중 내림",
            &s_conv,
            |p| p.symbol_weight_pct = d("10"),
            &[],
        ),
        case(
            "업종 비중 오름",
            &s,
            |p| p.sector_weight_pct = d("45"),
            &[RiskField::SectorWeight],
        ),
        case("업종 비중 내림", &s, |p| p.sector_weight_pct = d("30"), &[]),
        case(
            "자동 주문 한도 오름",
            &s,
            |p| p.auto_order_limit_pct = d("20"),
            &[RiskField::AutoOrderLimit],
        ),
        case(
            "자동 주문 한도 내림",
            &s,
            |p| p.auto_order_limit_pct = d("10"),
            &[],
        ),
        case(
            "최소 현금 내림",
            &s,
            |p| p.min_cash_pct = d("5"),
            &[RiskField::MinCash],
        ),
        case("최소 현금 오름", &s, |p| p.min_cash_pct = d("20"), &[]),
    ]);
}

// 검증: P15, E28
#[test]
fn counts_widen_only_when_raised() {
    let s = swing();
    check(vec![
        case(
            "최대 보유 수 위 끝 오름",
            &s,
            |p| p.max_positions = CountRange { low: 5, high: 10 },
            &[RiskField::MaxPositions],
        ),
        case(
            "최대 보유 수 위 끝 내림",
            &s,
            |p| p.max_positions = CountRange { low: 5, high: 6 },
            &[],
        ),
        case(
            "최대 보유 수 아래 끝만 오름(위 끝 그대로)",
            &s,
            |p| p.max_positions = CountRange { low: 7, high: 8 },
            &[],
        ),
        case(
            "일일 매매 횟수 오름",
            &s,
            |p| p.max_trades_per_day = 5,
            &[RiskField::MaxTradesPerDay],
        ),
        case("일일 매매 횟수 내림", &s, |p| p.max_trades_per_day = 2, &[]),
    ]);
}

// 검증: P15, E28
#[test]
fn conviction_weight_counts_symbol_weight_when_absent() {
    let s = swing(); // 종목 15%, 확신 없음
    let some20 = with(&s, |p| p.conviction_weight_pct = Some(d("20")));
    let some10 = with(&s, |p| p.conviction_weight_pct = Some(d("10")));
    check(vec![
        case(
            "없음(=15) → 20",
            &s,
            |p| p.conviction_weight_pct = Some(d("20")),
            &[RiskField::ConvictionWeight],
        ),
        case(
            "없음(=15) → 10",
            &s,
            |p| p.conviction_weight_pct = Some(d("10")),
            &[],
        ),
        case(
            "없음(=15) → 15 같음",
            &s,
            |p| p.conviction_weight_pct = Some(d("15")),
            &[],
        ),
        case(
            "20 → 없음(=15)",
            &some20,
            |p| p.conviction_weight_pct = None,
            &[],
        ),
        case(
            "10 → 없음(=15)",
            &some10,
            |p| p.conviction_weight_pct = None,
            &[RiskField::ConvictionWeight],
        ),
        case(
            "20 → 25",
            &some20,
            |p| p.conviction_weight_pct = Some(d("25")),
            &[RiskField::ConvictionWeight],
        ),
        // 확신 비중이 없으면 종목 비중으로 세므로 종목 비중을 올리면 둘 다 오른다(표의 unwrap_or 그대로).
        case(
            "확신 없음 + 종목 15 → 20",
            &s,
            |p| p.symbol_weight_pct = d("20"),
            &[RiskField::SymbolWeight, RiskField::ConvictionWeight],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn loss_streak_boundaries() {
    let s = swing(); // 4번 연속 · 거래일 2일
    let streak = |losses, cooldown| Some(LossStreakStop { losses, cooldown });
    let rest_of_day = with(&s, |p| p.loss_streak = streak(4, Cooldown::RestOfDay));
    let none = with(&s, |p| p.loss_streak = None);
    check(vec![
        case(
            "손실 횟수 오름",
            &s,
            |p| p.loss_streak = streak(5, Cooldown::TradingDays(2)),
            &[RiskField::LossStreak],
        ),
        case(
            "손실 횟수 내림",
            &s,
            |p| p.loss_streak = streak(3, Cooldown::TradingDays(2)),
            &[],
        ),
        case(
            "없어짐",
            &s,
            |p| p.loss_streak = None,
            &[RiskField::LossStreak],
        ),
        case(
            "없던 것이 생김",
            &none,
            |p| p.loss_streak = streak(4, Cooldown::TradingDays(2)),
            &[],
        ),
        case(
            "거래일 2일 → 그날만",
            &s,
            |p| p.loss_streak = streak(4, Cooldown::RestOfDay),
            &[RiskField::LossStreak],
        ),
        case(
            "그날만 → 거래일 1일",
            &rest_of_day,
            |p| p.loss_streak = streak(4, Cooldown::TradingDays(1)),
            &[],
        ),
        case(
            "거래일 2일 → 1일",
            &s,
            |p| p.loss_streak = streak(4, Cooldown::TradingDays(1)),
            &[RiskField::LossStreak],
        ),
        case(
            "거래일 2일 → 3일",
            &s,
            |p| p.loss_streak = streak(4, Cooldown::TradingDays(3)),
            &[],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn fixed_and_atr_stop_boundaries() {
    let s = swing(); // 고정 7~8%, ATR 2배(일봉)
    let range = |low: &str, high: &str| {
        Some(PctRange {
            low: d(low),
            high: d(high),
        })
    };
    let no_fixed = with(&s, |p| p.stop.fixed_loss_pct = None);
    let no_atr = with(&s, |p| p.stop.atr = None);
    let atr = |m: &str| {
        Some(AtrStop {
            multiple: d(m),
            bar: AtrBar::Day,
        })
    };
    check(vec![
        case(
            "고정 위 끝 오름",
            &s,
            |p| p.stop.fixed_loss_pct = range("7", "9"),
            &[RiskField::FixedLossStop],
        ),
        case(
            "고정 아래 끝 내려감",
            &s,
            |p| p.stop.fixed_loss_pct = range("6", "8"),
            &[RiskField::FixedLossStop],
        ),
        case(
            "고정 위 끝 내림",
            &s,
            |p| p.stop.fixed_loss_pct = range("7", "7.5"),
            &[],
        ),
        case(
            "고정 아래 끝 오름",
            &s,
            |p| p.stop.fixed_loss_pct = range("7.5", "8"),
            &[],
        ),
        case(
            "고정 없어짐",
            &s,
            |p| p.stop.fixed_loss_pct = None,
            &[RiskField::FixedLossStop],
        ),
        case(
            "고정 없던 것이 생김",
            &no_fixed,
            |p| p.stop.fixed_loss_pct = range("7", "8"),
            &[],
        ),
        case(
            "ATR 배수 오름",
            &s,
            |p| p.stop.atr = atr("3"),
            &[RiskField::AtrStop],
        ),
        case("ATR 배수 내림", &s, |p| p.stop.atr = atr("1.5"), &[]),
        case(
            "ATR 없어짐",
            &s,
            |p| p.stop.atr = None,
            &[RiskField::AtrStop],
        ),
        case(
            "ATR 없던 것이 생김",
            &no_atr,
            |p| p.stop.atr = atr("2"),
            &[],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn thesis_break_and_flatten_turned_off_widen() {
    let s = swing();
    let on = with(&s, |p| {
        p.stop.thesis_break = true;
        p.stop.flatten_before_close = true;
    });
    let off = with(&s, |p| {
        p.stop.thesis_break = false;
        p.stop.flatten_before_close = false;
    });
    check(vec![
        case(
            "논리 손절 끔",
            &on,
            |p| p.stop.thesis_break = false,
            &[RiskField::ThesisBreakStop],
        ),
        case("논리 손절 켬", &off, |p| p.stop.thesis_break = true, &[]),
        case(
            "장 마감 청산 끔",
            &on,
            |p| p.stop.flatten_before_close = false,
            &[RiskField::FlattenBeforeClose],
        ),
        case(
            "장 마감 청산 켬",
            &off,
            |p| p.stop.flatten_before_close = true,
            &[],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn max_holding_longer_widens() {
    let s = swing(); // 거래일 20일
    let same_day = with(&s, |p| p.max_holding = MaxHolding::SameDay);
    let unlimited = with(&s, |p| p.max_holding = MaxHolding::Unlimited);
    check(vec![
        case(
            "20일 → 30일",
            &s,
            |p| p.max_holding = MaxHolding::TradingDays(30),
            &[RiskField::MaxHolding],
        ),
        case(
            "20일 → 정하지 않음",
            &s,
            |p| p.max_holding = MaxHolding::Unlimited,
            &[RiskField::MaxHolding],
        ),
        case(
            "20일 → 10일",
            &s,
            |p| p.max_holding = MaxHolding::TradingDays(10),
            &[],
        ),
        case(
            "20일 → 그날",
            &s,
            |p| p.max_holding = MaxHolding::SameDay,
            &[],
        ),
        case(
            "그날 → 1일",
            &same_day,
            |p| p.max_holding = MaxHolding::TradingDays(1),
            &[RiskField::MaxHolding],
        ),
        case(
            "정하지 않음 → 100일",
            &unlimited,
            |p| p.max_holding = MaxHolding::TradingDays(100),
            &[],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn chase_limit_raised_or_removed_widens() {
    let s = swing(); // 2%
    let none = with(&s, |p| p.chase_limit_pct = None);
    check(vec![
        case(
            "추격 폭 오름",
            &s,
            |p| p.chase_limit_pct = Some(d("3")),
            &[RiskField::ChaseLimit],
        ),
        case(
            "추격 폭 없어짐",
            &s,
            |p| p.chase_limit_pct = None,
            &[RiskField::ChaseLimit],
        ),
        case(
            "추격 폭 내림",
            &s,
            |p| p.chase_limit_pct = Some(d("1")),
            &[],
        ),
        case(
            "추격 폭 없던 것이 생김",
            &none,
            |p| p.chase_limit_pct = Some(d("1")),
            &[],
        ),
    ]);
}

// 검증: P15, P14, E28
#[test]
fn promotion_loosened_widens() {
    let s = swing(); // 30건 · 20일 · 손익 > 0 · 표본 밖 > 0
    let strict = with(&s, |p| {
        p.promotion.net_pnl_above = d("5");
        p.promotion.out_of_sample_pnl_above = Some(d("5"));
    });
    let no_oos = with(&s, |p| p.promotion.out_of_sample_pnl_above = None);
    check(vec![
        case(
            "매매 건수 내림",
            &s,
            |p| p.promotion.min_trades = 20,
            &[RiskField::Promotion],
        ),
        case("매매 건수 오름", &s, |p| p.promotion.min_trades = 40, &[]),
        case(
            "거래일 내림",
            &s,
            |p| p.promotion.min_trading_days = 10,
            &[RiskField::Promotion],
        ),
        case(
            "거래일 오름",
            &s,
            |p| p.promotion.min_trading_days = 30,
            &[],
        ),
        case(
            "비용 뺀 손익 기준 내림",
            &strict,
            |p| p.promotion.net_pnl_above = d("0"),
            &[RiskField::Promotion],
        ),
        case(
            "비용 뺀 손익 기준 오름",
            &s,
            |p| p.promotion.net_pnl_above = d("5"),
            &[],
        ),
        case(
            "표본 밖 기준 내림",
            &strict,
            |p| p.promotion.out_of_sample_pnl_above = Some(d("0")),
            &[RiskField::Promotion],
        ),
        case(
            "표본 밖 기준 없어짐",
            &s,
            |p| p.promotion.out_of_sample_pnl_above = None,
            &[RiskField::Promotion],
        ),
        case(
            "표본 밖 기준 없던 것이 생김",
            &no_oos,
            |p| p.promotion.out_of_sample_pnl_above = Some(d("0")),
            &[],
        ),
        // 여러 기준이 함께 느슨해져도 칸은 하나.
        case(
            "건수 · 거래일 함께 내림",
            &s,
            |p| {
                p.promotion.min_trades = 10;
                p.promotion.min_trading_days = 5;
            },
            &[RiskField::Promotion],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn fields_outside_table_never_widen() {
    let s = swing();
    check(vec![
        case(
            "유니버스",
            &s,
            |p| {
                p.universe.markets = vec![Market::Kr, Market::Us];
                p.universe.min_market_cap = None;
                p.universe.min_turnover = None;
            },
            &[],
        ),
        case(
            "거래 시간대 넓힘",
            &s,
            |p| {
                p.trading_window = TradingWindow {
                    skip_open_minutes: 1,
                    after_market: true,
                }
            },
            &[],
        ),
        case(
            "진입 신호",
            &s,
            |p| {
                p.entry_signals = vec![
                    EntrySignal::Breakout,
                    EntrySignal::Pullback,
                    EntrySignal::ValueRange,
                    EntrySignal::Event,
                ]
            },
            &[],
        ),
        case(
            "트리거",
            &s,
            |p| {
                p.triggers = vec![
                    TriggerKind::Quote,
                    TriggerKind::Disclosure,
                    TriggerKind::News,
                    TriggerKind::Schedule,
                ]
            },
            &[],
        ),
        case(
            "리밸런싱",
            &s,
            |p| {
                p.rebalance = Some(Rebalance {
                    every_trading_days: 20,
                    drift_pct: d("5"),
                })
            },
            &[],
        ),
        case(
            "익절",
            &s,
            |p| {
                p.take_profit = TakeProfitRule {
                    r_target: Some(d("5")),
                    partial_at_target: false,
                    trailing: false,
                    valuation_exit: false,
                    rebalance_exit: false,
                }
            },
            &[],
        ),
        case(
            "승인 대기 짧게",
            &s,
            |p| {
                p.approval_wait = ApprovalWait {
                    stop_loss: Duration::from_secs(10),
                    take_profit: Duration::from_secs(10),
                    buy: Duration::from_secs(10),
                }
            },
            &[],
        ),
        case(
            "미체결 취소 장 마감까지",
            &s,
            |p| p.unfilled_cancel = UnfilledCancel::AtClose,
            &[],
        ),
    ]);
}

// 검증: P15, E28
#[test]
fn widened_fields_are_in_order_without_duplicates() {
    let s = swing();
    let after = with(&s, |p| {
        p.promotion.min_trades = 10;
        p.sector_weight_pct = d("45");
        p.max_trades_per_day = 9;
        p.risk_per_trade_pct = d("2");
        p.stop.atr = None;
        p.min_cash_pct = d("0");
    });
    assert_eq!(
        widened_fields(&s, &after),
        vec![
            RiskField::RiskPerTrade,
            RiskField::SectorWeight,
            RiskField::MinCash,
            RiskField::MaxTradesPerDay,
            RiskField::AtrStop,
            RiskField::Promotion,
        ]
    );
    assert!(increases_risk(&s, &after));
    // 같은 값끼리는 빈 목록.
    for style in Style::ALL {
        let p = preset(style);
        assert_eq!(widened_fields(&p, &p), Vec::<RiskField>::new(), "{style:?}");
        assert!(!increases_risk(&p, &p), "{style:?}");
    }
}

// ── plan_adjust ──

// 검증: P15, C14, E5
#[test]
fn same_values_apply_now_for_every_preset() {
    for style in Style::ALL {
        let current = preset(style);
        let changes = same_changes(&current);
        assert_eq!(
            plan_adjust(&current, &changes),
            Ok(AdjustPlan::ApplyNow(current.clone())),
            "{style:?}"
        );
        assert_eq!(
            plan_adjust(&current, &[]),
            Ok(AdjustPlan::ApplyNow(current.clone())),
            "{style:?} 빈 묶음"
        );
    }
}

// 검증: P15, C14, E5
#[test]
fn narrowing_only_applies_now() {
    let current = swing();
    let changes = [
        Change::RiskPerTradePct(d("0.5")),
        Change::MinCashPct(d("20")),
        Change::MaxTradesPerDay(2),
        Change::ChaseLimitPct(Some(d("1"))),
    ];
    let want = adjust(&current, &changes).unwrap();
    assert_eq!(
        plan_adjust(&current, &changes),
        Ok(AdjustPlan::ApplyNow(want))
    );
}

// 검증: P15, E28, C14
#[test]
fn widening_needs_approval_with_after_value() {
    let current = swing();
    let before = current.clone();
    let changes = [Change::DailyLossLimitPct(d("4"))];
    let want = adjust(&current, &changes).unwrap();
    assert_eq!(
        plan_adjust(&current, &changes),
        Ok(AdjustPlan::NeedsApproval {
            after: want,
            widened: vec![RiskField::DailyLossLimit],
        })
    );
    assert_eq!(current, before, "plan_adjust 가 지금 값을 바꿈");
}

// 검증: P15, E28, C14
#[test]
fn mixed_bundle_needs_approval_as_a_whole() {
    let current = swing();
    // R% 내림(줄임) + 업종 비중 오름(키움) + 최소 현금 오름(줄임).
    let changes = [
        Change::RiskPerTradePct(d("0.5")),
        Change::SectorWeightPct(d("45")),
        Change::MinCashPct(d("20")),
    ];
    let want = adjust(&current, &changes).unwrap();
    assert_eq!(want.risk_per_trade_pct, d("0.5"));
    match plan_adjust(&current, &changes) {
        Ok(AdjustPlan::NeedsApproval { after, widened }) => {
            assert_eq!(after, want, "줄이는 칸도 묶음에 그대로 담겨야 함");
            assert_eq!(widened, vec![RiskField::SectorWeight], "키운 칸만");
        }
        other => panic!("섞인 묶음은 통째로 승인 대기여야 함: {other:?}"),
    }
}

// 검증: P15, E5, E28, C14, V28
#[test]
fn over_cap_widening_is_rejected_not_queued() {
    let current = swing();
    let before = current.clone();
    let err = match plan_adjust(&current, &[Change::RiskPerTradePct(d("3"))]) {
        Err(e) => e,
        Ok(plan) => panic!("상한을 넘는 변경은 승인 대기가 아니라 거부여야 함: {plan:?}"),
    };
    assert_eq!(err.violations.len(), 1);
    assert_eq!(err.violations[0].field, CappedField::RiskPerTrade);
    assert_eq!(err.violations[0].requested, d("3"));
    assert_eq!(err.violations[0].cap, d("2"));
    assert!(err.to_string().contains("2%"), "V28: {err}");
    assert_eq!(current, before, "거부된 조정이 지금 값을 바꿈");
    assert_eq!(
        Err(err),
        adjust(&current, &[Change::RiskPerTradePct(d("3"))])
    );
}

// 검증: P15, E5, E28, C14
#[test]
fn cap_edge_is_queued_and_just_over_is_rejected() {
    let current = swing();
    assert_eq!(
        plan_adjust(&current, &[Change::RiskPerTradePct(d("2"))]),
        Ok(AdjustPlan::NeedsApproval {
            after: with(&current, |p| p.risk_per_trade_pct = d("2")),
            widened: vec![RiskField::RiskPerTrade],
        })
    );
    assert!(plan_adjust(&current, &[Change::RiskPerTradePct(d("2.0001"))]).is_err());
    // 줄이는 칸이 섞여 있어도 하나라도 상한을 넘으면 전체 거부.
    assert!(
        plan_adjust(
            &current,
            &[
                Change::MinCashPct(d("20")),
                Change::SectorWeightPct(d("51"))
            ]
        )
        .is_err()
    );
}

// 검증: P15, E28
#[test]
fn lower_bound_violation_is_rejected_even_if_narrowing() {
    let current = swing();
    // 0% 는 위험을 줄이는 쪽이지만 아래쪽 경계 위반이라 거부(2차).
    let got = plan_adjust(&current, &[Change::RiskPerTradePct(d("0"))]);
    assert_eq!(
        got,
        adjust(&current, &[Change::RiskPerTradePct(d("0"))]).map(AdjustPlan::ApplyNow)
    );
    assert!(got.is_err());
}

// ── 속성: plan_adjust 의 결과 값은 언제나 adjust 의 결과와 같다 ──

fn pool(p: &RiskParams) -> Vec<Change> {
    let mut out = same_changes(p);
    out.extend([
        Change::RiskPerTradePct(d("0.5")),
        Change::RiskPerTradePct(d("2")),
        Change::RiskPerTradePct(d("3")),
        Change::RiskPerTradePct(d("0")),
        Change::DailyLossLimitPct(d("1")),
        Change::DailyLossLimitPct(d("6")),
        Change::MaxDrawdownPct(d("5")),
        Change::MaxDrawdownPct(d("20")),
        Change::SymbolWeightPct(d("5")),
        Change::SymbolWeightPct(d("25")),
        Change::SymbolWeightPct(d("30")),
        Change::ConvictionWeightPct(None),
        Change::ConvictionWeightPct(Some(d("25"))),
        Change::SectorWeightPct(d("10")),
        Change::SectorWeightPct(d("50")),
        Change::SectorWeightPct(d("60")),
        Change::MinCashPct(d("0")),
        Change::MinCashPct(d("30")),
        Change::AutoOrderLimitPct(d("5")),
        Change::AutoOrderLimitPct(d("30")),
        Change::MaxTradesPerDay(1),
        Change::MaxTradesPerDay(10),
        Change::MaxPositions(CountRange { low: 1, high: 30 }),
        Change::MaxPositions(CountRange { low: 9, high: 2 }),
        Change::LossStreak(None),
        Change::LossStreak(Some(LossStreakStop {
            losses: 2,
            cooldown: Cooldown::TradingDays(5),
        })),
        Change::MaxHolding(MaxHolding::Unlimited),
        Change::MaxHolding(MaxHolding::SameDay),
        Change::ChaseLimitPct(None),
        Change::ChaseLimitPct(Some(d("5"))),
        Change::TradingWindow(TradingWindow {
            skip_open_minutes: 1,
            after_market: true,
        }),
        Change::UnfilledCancel(UnfilledCancel::AtClose),
    ]);
    let mut loose = p.promotion.clone();
    loose.min_trades = 1;
    loose.out_of_sample_pnl_above = None;
    out.push(Change::Promotion(loose));
    let mut stop = p.stop.clone();
    stop.atr = None;
    stop.thesis_break = !stop.thesis_break;
    out.push(Change::Stop(stop));
    out
}

proptest! {
    // 검증: P15, C14, E5, E28
    #[test]
    fn plan_adjust_value_matches_adjust(
        style in 0usize..3,
        picks in prop::collection::vec(0usize..1000, 0..8),
    ) {
        let current = preset(Style::ALL[style]);
        let before = current.clone();
        let candidates = pool(&current);
        let changes: Vec<Change> =
            picks.iter().map(|i| candidates[i % candidates.len()].clone()).collect();
        let plan = plan_adjust(&current, &changes);
        let plain = adjust(&current, &changes);
        prop_assert_eq!(&current, &before);
        match (plan, plain) {
            (Ok(AdjustPlan::ApplyNow(v)), Ok(w)) => {
                prop_assert_eq!(&v, &w);
                prop_assert!(widened_fields(&current, &w).is_empty());
            }
            (Ok(AdjustPlan::NeedsApproval { after, widened }), Ok(w)) => {
                prop_assert_eq!(&after, &w);
                prop_assert!(!widened.is_empty());
                prop_assert_eq!(widened, widened_fields(&current, &w));
            }
            (Err(a), Err(b)) => prop_assert_eq!(a, b),
            (a, b) => prop_assert!(false, "plan_adjust {:?} / adjust {:?}", a, b),
        }
    }
}
