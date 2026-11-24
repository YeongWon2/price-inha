//! 12차(2026-11-25, 결정 17): 템플릿 화면의 아래 함수 — 템플릿 보기 · 모자란 승격 기준 · 판 사이 차이 ·
//! 실전 템플릿 멈추기의 전이. 지시서 docs/dev/modules/template.md "12차 작업", additions.md C28 · E58 · T37.

use pi_template::{
    ALLOWED_TRANSITIONS, AbsoluteCaps, Change, Cooldown, Criterion, CriterionLine, FieldChange,
    LossStreakStop, MaxHolding, PctRange, PromotionCriteria, PromotionStats, RiskField, RiskParams,
    Style, TemplateState, TransitionReason, UnfilledCancel, adjust, diff_params, preset,
    promotion_lines, template_view, widened_fields,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::time::Duration;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

fn val<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

/// 보기 칸 순서(지시서 9차 (b) 4 표의 순서, 손절은 네 칸).
const KEYS: [&str; 26] = [
    "risk_per_trade_pct",
    "daily_loss_limit_pct",
    "max_drawdown_pct",
    "symbol_weight_pct",
    "conviction_weight_pct",
    "sector_weight_pct",
    "auto_order_limit_pct",
    "min_cash_pct",
    "max_positions",
    "max_trades_per_day",
    "loss_streak",
    "stop.fixed_loss_pct",
    "stop.atr",
    "stop.thesis_break",
    "stop.flatten_before_close",
    "max_holding",
    "chase_limit_pct",
    "promotion",
    "universe",
    "trading_window",
    "entry_signals",
    "triggers",
    "rebalance",
    "take_profit",
    "approval_wait",
    "unfilled_cancel",
];

/// 칸 이름과 지금 값(그 칸을 json 으로 낸 것).
fn expected(p: &RiskParams) -> Vec<(&'static str, Value)> {
    vec![
        ("risk_per_trade_pct", val(&p.risk_per_trade_pct)),
        ("daily_loss_limit_pct", val(&p.daily_loss_limit_pct)),
        ("max_drawdown_pct", val(&p.max_drawdown_pct)),
        ("symbol_weight_pct", val(&p.symbol_weight_pct)),
        ("conviction_weight_pct", val(&p.conviction_weight_pct)),
        ("sector_weight_pct", val(&p.sector_weight_pct)),
        ("auto_order_limit_pct", val(&p.auto_order_limit_pct)),
        ("min_cash_pct", val(&p.min_cash_pct)),
        ("max_positions", val(&p.max_positions)),
        ("max_trades_per_day", val(&p.max_trades_per_day)),
        ("loss_streak", val(&p.loss_streak)),
        ("stop.fixed_loss_pct", val(&p.stop.fixed_loss_pct)),
        ("stop.atr", val(&p.stop.atr)),
        ("stop.thesis_break", val(&p.stop.thesis_break)),
        (
            "stop.flatten_before_close",
            val(&p.stop.flatten_before_close),
        ),
        ("max_holding", val(&p.max_holding)),
        ("chase_limit_pct", val(&p.chase_limit_pct)),
        ("promotion", val(&p.promotion)),
        ("universe", val(&p.universe)),
        ("trading_window", val(&p.trading_window)),
        ("entry_signals", val(&p.entry_signals)),
        ("triggers", val(&p.triggers)),
        ("rebalance", val(&p.rebalance)),
        ("take_profit", val(&p.take_profit)),
        ("approval_wait", val(&p.approval_wait)),
        ("unfilled_cancel", val(&p.unfilled_cancel)),
    ]
}

// ───────────────────────── 1. 템플릿 보기 모양 ─────────────────────────

// 검증: C4, V27
#[test]
fn view_lists_every_field_in_table_order_for_each_preset() {
    for style in Style::ALL {
        let view = template_view(&preset(style));
        let keys: Vec<&str> = view.iter().map(|f| f.key).collect();
        assert_eq!(keys, KEYS.to_vec(), "{style:?}");
    }
}

// 검증: C4, V27
#[test]
fn view_values_are_the_current_values_for_each_preset() {
    for style in Style::ALL {
        let p = preset(style);
        let view = template_view(&p);
        let got: Vec<(&str, Value)> = view.iter().map(|f| (f.key, f.value.clone())).collect();
        assert_eq!(got, expected(&p), "{style:?}");
    }
}

// 검증: C14, E5, V28
#[test]
fn view_shows_caps_only_on_the_six_capped_fields() {
    let caps = AbsoluteCaps::FIXED;
    let want = |key: &str| -> Option<Decimal> {
        match key {
            "risk_per_trade_pct" => Some(caps.risk_per_trade_pct),
            "daily_loss_limit_pct" => Some(caps.daily_loss_pct),
            "max_drawdown_pct" => Some(caps.max_drawdown_pct),
            "symbol_weight_pct" => Some(caps.symbol_weight_pct),
            "conviction_weight_pct" => Some(caps.symbol_weight_pct),
            "sector_weight_pct" => Some(caps.sector_weight_pct),
            _ => None,
        }
    };
    for style in Style::ALL {
        let view = template_view(&preset(style));
        for f in &view {
            assert_eq!(f.cap, want(f.key), "{style:?} {}", f.key);
        }
        assert_eq!(view.iter().filter(|f| f.cap.is_some()).count(), 6);
    }
    // 끝자리까지: R 2 · 일일 손실 5 · 낙폭 20 · 종목 25 · 확신 25 · 업종 50.
    let view = template_view(&preset(Style::Swing));
    let cap = |key: &str| view.iter().find(|f| f.key == key).unwrap().cap.unwrap();
    assert_eq!(cap("risk_per_trade_pct"), d("2"));
    assert_eq!(cap("daily_loss_limit_pct"), d("5"));
    assert_eq!(cap("max_drawdown_pct"), d("20"));
    assert_eq!(cap("symbol_weight_pct"), d("25"));
    assert_eq!(cap("conviction_weight_pct"), d("25"));
    assert_eq!(cap("sector_weight_pct"), d("50"));
}

// 검증: C14
#[test]
fn view_marks_every_field_editable_within_its_cap() {
    for style in Style::ALL {
        for f in template_view(&preset(style)) {
            assert!(f.editable, "{style:?} {}", f.key);
        }
    }
}

// 검증: C14, E5
#[test]
fn view_follows_adjusted_values_and_keeps_the_cap() {
    let before = preset(Style::Swing);
    let after = adjust(
        &before,
        &[
            Change::RiskPerTradePct(d("1.5")),
            Change::ConvictionWeightPct(Some(d("20"))),
        ],
    )
    .unwrap();
    let view = template_view(&after);
    let find = |key: &str| view.iter().find(|f| f.key == key).unwrap().clone();
    assert_eq!(find("risk_per_trade_pct").value, val(&d("1.5")));
    assert_eq!(find("risk_per_trade_pct").cap, Some(d("2")));
    assert_eq!(find("conviction_weight_pct").value, val(&Some(d("20"))));
    // 바뀌지 않은 칸은 그대로.
    assert_eq!(
        find("max_drawdown_pct").value,
        val(&before.max_drawdown_pct)
    );
    // 확신 종목 비중이 없는 값은 null 이지만 상한 칸이다.
    let mut none = preset(Style::Swing);
    none.conviction_weight_pct = None;
    let view = template_view(&none);
    let f = view
        .iter()
        .find(|f| f.key == "conviction_weight_pct")
        .unwrap();
    assert_eq!(f.value, Value::Null);
    assert_eq!(f.cap, Some(d("25")));
}

// ───────────────────────── 2. 모자란 승격 기준 ─────────────────────────

fn criteria() -> PromotionCriteria {
    PromotionCriteria {
        min_trades: 30,
        min_trading_days: 20,
        net_pnl_above: d("0"),
        max_drawdown_pct: d("10"),
        out_of_sample_pnl_above: Some(d("0")),
    }
}

fn passing_stats() -> PromotionStats {
    PromotionStats {
        trades: 31,
        trading_days: 21,
        net_pnl: d("1000"),
        max_drawdown_pct: d("4"),
        out_of_sample_pnl: Some(d("50")),
    }
}

const ORDER: [Criterion; 5] = [
    Criterion::Trades,
    Criterion::TradingDays,
    Criterion::NetPnl,
    Criterion::MaxDrawdown,
    Criterion::OutOfSample,
];

fn line(lines: &[CriterionLine], c: Criterion) -> CriterionLine {
    lines.iter().find(|l| l.criterion == c).unwrap().clone()
}

// 검증: P14, E26
#[test]
fn promotion_lines_when_everything_is_met() {
    let lines = promotion_lines(&criteria(), &passing_stats());
    let order: Vec<Criterion> = lines.iter().map(|l| l.criterion).collect();
    assert_eq!(order, ORDER.to_vec());
    assert!(lines.iter().all(|l| l.met));
    assert_eq!(
        lines[0],
        CriterionLine {
            criterion: Criterion::Trades,
            need: d("30"),
            have: Some(d("31")),
            met: true
        }
    );
    assert_eq!(
        lines[1],
        CriterionLine {
            criterion: Criterion::TradingDays,
            need: d("20"),
            have: Some(d("21")),
            met: true
        }
    );
    assert_eq!(
        lines[2],
        CriterionLine {
            criterion: Criterion::NetPnl,
            need: d("0"),
            have: Some(d("1000")),
            met: true
        }
    );
    assert_eq!(
        lines[3],
        CriterionLine {
            criterion: Criterion::MaxDrawdown,
            need: d("10"),
            have: Some(d("4")),
            met: true
        }
    );
    assert_eq!(
        lines[4],
        CriterionLine {
            criterion: Criterion::OutOfSample,
            need: d("0"),
            have: Some(d("50")),
            met: true
        }
    );
}

// 검증: P14, E26
#[test]
fn promotion_lines_one_criterion_short() {
    let mut s = passing_stats();
    s.trades = 12;
    let lines = promotion_lines(&criteria(), &s);
    assert_eq!(lines.len(), 5);
    let short: Vec<Criterion> = lines
        .iter()
        .filter(|l| !l.met)
        .map(|l| l.criterion)
        .collect();
    assert_eq!(short, vec![Criterion::Trades]);
    let t = line(&lines, Criterion::Trades);
    assert_eq!((t.need, t.have), (d("30"), Some(d("12"))));
}

// 검증: P14, E26, E58
#[test]
fn promotion_lines_several_short_each_with_need_and_have() {
    let s = PromotionStats {
        trades: 5,
        trading_days: 3,
        net_pnl: d("-200"),
        max_drawdown_pct: d("12.5"),
        out_of_sample_pnl: Some(d("-1")),
    };
    let lines = promotion_lines(&criteria(), &s);
    assert!(lines.iter().all(|l| !l.met));
    assert_eq!(
        line(&lines, Criterion::TradingDays),
        CriterionLine {
            criterion: Criterion::TradingDays,
            need: d("20"),
            have: Some(d("3")),
            met: false
        }
    );
    assert_eq!(
        line(&lines, Criterion::NetPnl),
        CriterionLine {
            criterion: Criterion::NetPnl,
            need: d("0"),
            have: Some(d("-200")),
            met: false
        }
    );
    assert_eq!(
        line(&lines, Criterion::MaxDrawdown),
        CriterionLine {
            criterion: Criterion::MaxDrawdown,
            need: d("10"),
            have: Some(d("12.5")),
            met: false
        }
    );
    assert_eq!(
        line(&lines, Criterion::OutOfSample),
        CriterionLine {
            criterion: Criterion::OutOfSample,
            need: d("0"),
            have: Some(d("-1")),
            met: false
        }
    );
}

// 검증: P14, E26
#[test]
fn promotion_lines_boundaries_match_the_judgement() {
    // 건수 · 거래일은 같으면 채움, 손익은 같으면 못 채움, 낙폭은 같으면 채움.
    let s = PromotionStats {
        trades: 30,
        trading_days: 20,
        net_pnl: d("0"),
        max_drawdown_pct: d("10"),
        out_of_sample_pnl: Some(d("0")),
    };
    let lines = promotion_lines(&criteria(), &s);
    assert!(line(&lines, Criterion::Trades).met);
    assert!(line(&lines, Criterion::TradingDays).met);
    assert!(!line(&lines, Criterion::NetPnl).met);
    assert!(line(&lines, Criterion::MaxDrawdown).met);
    assert!(!line(&lines, Criterion::OutOfSample).met);
}

// 검증: P14, E26
#[test]
fn promotion_lines_skip_out_of_sample_when_not_required() {
    let mut c = criteria();
    c.out_of_sample_pnl_above = None;
    let mut s = passing_stats();
    s.out_of_sample_pnl = None;
    let lines = promotion_lines(&c, &s);
    let order: Vec<Criterion> = lines.iter().map(|l| l.criterion).collect();
    assert_eq!(order, ORDER[..4].to_vec());
    assert!(lines.iter().all(|l| l.met));
}

// 검증: P14, E26
#[test]
fn promotion_lines_out_of_sample_not_yet_available_is_short_with_no_have() {
    let mut s = passing_stats();
    s.out_of_sample_pnl = None;
    let lines = promotion_lines(&criteria(), &s);
    let o = line(&lines, Criterion::OutOfSample);
    assert_eq!(o.need, d("0"));
    assert_eq!(o.have, None);
    assert!(!o.met);
}

// 검증: P14, E26
#[test]
fn promotion_lines_agree_with_evaluate() {
    let c = criteria();
    let mut cases = vec![passing_stats()];
    for tweak in 0..6 {
        let mut s = passing_stats();
        match tweak {
            0 => s.trades = 29,
            1 => s.trading_days = 19,
            2 => s.net_pnl = d("0"),
            3 => s.max_drawdown_pct = d("10.0001"),
            4 => s.out_of_sample_pnl = None,
            _ => {
                s.trades = 0;
                s.net_pnl = d("-5");
            }
        }
        cases.push(s);
    }
    for s in cases {
        let short: Vec<Criterion> = promotion_lines(&c, &s)
            .iter()
            .filter(|l| !l.met)
            .map(|l| l.criterion)
            .collect();
        match c.evaluate(&s) {
            pi_template::Verdict::Passed => assert!(short.is_empty(), "{s:?}"),
            pi_template::Verdict::Short(want) => assert_eq!(short, want, "{s:?}"),
        }
    }
}

// ───────────────────────── 3. 판 사이 차이 ─────────────────────────

/// 바탕: 칸마다 값이 있는 스윙 프리셋 사본.
fn base() -> RiskParams {
    let mut p = preset(Style::Swing);
    p.conviction_weight_pct = Some(d("10"));
    p.loss_streak = Some(LossStreakStop {
        losses: 3,
        cooldown: Cooldown::TradingDays(2),
    });
    p.stop.fixed_loss_pct = Some(PctRange {
        low: d("2"),
        high: d("3"),
    });
    p.stop.atr = Some(pi_template::AtrStop {
        multiple: d("2"),
        bar: pi_template::AtrBar::Minute,
    });
    p.stop.thesis_break = true;
    p.stop.flatten_before_close = true;
    p.max_holding = MaxHolding::TradingDays(5);
    p.chase_limit_pct = Some(d("1"));
    p.max_positions = pi_template::CountRange { low: 2, high: 5 };
    p.max_trades_per_day = 5;
    p.promotion.out_of_sample_pnl_above = Some(d("0"));
    // 위험과 무관한 칸도 값을 정해 둔다(아래 시험이 바꿔서 달라지도록).
    p.universe.min_market_cap = None;
    p.trading_window.skip_open_minutes = 0;
    p.entry_signals = vec![pi_template::EntrySignal::Breakout];
    p.triggers = vec![pi_template::TriggerKind::Quote];
    p.rebalance = None;
    p.take_profit.trailing = false;
    p.approval_wait.buy = Duration::from_secs(300);
    p.unfilled_cancel = UnfilledCancel::After(Duration::from_secs(600));
    p
}

// 검증: C14
#[test]
fn diff_of_equal_values_is_empty() {
    for style in Style::ALL {
        let p = preset(style);
        assert!(diff_params(&p, &p.clone()).is_empty(), "{style:?}");
    }
}

// 검증: C14
#[test]
fn diff_lists_only_the_changed_field_with_before_and_after() {
    let before = base();
    let mut after = before.clone();
    after.risk_per_trade_pct = before.risk_per_trade_pct - d("0.25");
    let got = diff_params(&before, &after);
    assert_eq!(
        got,
        vec![FieldChange {
            key: "risk_per_trade_pct",
            before: val(&before.risk_per_trade_pct),
            after: val(&after.risk_per_trade_pct),
            widened: false,
        }]
    );
}

// 검증: C14, P15
#[test]
fn diff_lists_several_changes_in_view_order() {
    let before = base();
    let mut after = before.clone();
    // 일부러 뒤 칸부터 바꾼다.
    after.unfilled_cancel = UnfilledCancel::After(Duration::from_secs(123));
    after.promotion.min_trades = before.promotion.min_trades - 1;
    after.stop.thesis_break = false;
    after.min_cash_pct = before.min_cash_pct + d("1");
    after.risk_per_trade_pct = before.risk_per_trade_pct + d("0.1");
    let got = diff_params(&before, &after);
    let keys: Vec<&str> = got.iter().map(|c| c.key).collect();
    assert_eq!(
        keys,
        vec![
            "risk_per_trade_pct",
            "min_cash_pct",
            "stop.thesis_break",
            "promotion",
            "unfilled_cancel"
        ]
    );
    let widened: Vec<(&str, bool)> = got.iter().map(|c| (c.key, c.widened)).collect();
    assert_eq!(
        widened,
        vec![
            ("risk_per_trade_pct", true),
            ("min_cash_pct", false), // 현금을 늘림 = 줄임
            ("stop.thesis_break", true),
            ("promotion", true),
            ("unfilled_cancel", false),
        ]
    );
    let t = got.iter().find(|c| c.key == "stop.thesis_break").unwrap();
    assert_eq!(
        (t.before.clone(), t.after.clone()),
        (json!(true), json!(false))
    );
}

type Setter = fn(&mut RiskParams);

/// (칸 이름, 넓힘 표의 칸, 안전한 쪽, 위험한 쪽). 안전 → 위험은 넓힘, 위험 → 안전은 좁힘.
fn widening_cases() -> Vec<(&'static str, RiskField, Setter, Setter)> {
    fn set_streak(p: &mut RiskParams, losses: u32, cooldown: Cooldown) {
        p.loss_streak = Some(LossStreakStop { losses, cooldown });
    }
    vec![
        (
            "risk_per_trade_pct",
            RiskField::RiskPerTrade,
            |p| p.risk_per_trade_pct = d("1"),
            |p| p.risk_per_trade_pct = d("1.5"),
        ),
        (
            "daily_loss_limit_pct",
            RiskField::DailyLossLimit,
            |p| p.daily_loss_limit_pct = d("3"),
            |p| p.daily_loss_limit_pct = d("4"),
        ),
        (
            "max_drawdown_pct",
            RiskField::MaxDrawdown,
            |p| p.max_drawdown_pct = d("10"),
            |p| p.max_drawdown_pct = d("15"),
        ),
        (
            "symbol_weight_pct",
            RiskField::SymbolWeight,
            |p| p.symbol_weight_pct = d("10"),
            |p| p.symbol_weight_pct = d("15"),
        ),
        (
            "conviction_weight_pct",
            RiskField::ConvictionWeight,
            |p| p.conviction_weight_pct = Some(d("10")),
            |p| p.conviction_weight_pct = Some(d("15")),
        ),
        (
            "sector_weight_pct",
            RiskField::SectorWeight,
            |p| p.sector_weight_pct = d("30"),
            |p| p.sector_weight_pct = d("40"),
        ),
        (
            "auto_order_limit_pct",
            RiskField::AutoOrderLimit,
            |p| p.auto_order_limit_pct = d("5"),
            |p| p.auto_order_limit_pct = d("8"),
        ),
        (
            "min_cash_pct",
            RiskField::MinCash,
            |p| p.min_cash_pct = d("20"),
            |p| p.min_cash_pct = d("10"),
        ),
        (
            "max_positions",
            RiskField::MaxPositions,
            |p| p.max_positions = pi_template::CountRange { low: 2, high: 5 },
            |p| p.max_positions = pi_template::CountRange { low: 2, high: 6 },
        ),
        (
            "max_trades_per_day",
            RiskField::MaxTradesPerDay,
            |p| p.max_trades_per_day = 5,
            |p| p.max_trades_per_day = 6,
        ),
        (
            "loss_streak",
            RiskField::LossStreak,
            |p| set_streak(p, 3, Cooldown::TradingDays(3)),
            |p| set_streak(p, 3, Cooldown::RestOfDay),
        ),
        (
            "loss_streak",
            RiskField::LossStreak,
            |p| set_streak(p, 2, Cooldown::TradingDays(2)),
            |p| set_streak(p, 3, Cooldown::TradingDays(2)),
        ),
        (
            "loss_streak",
            RiskField::LossStreak,
            |p| set_streak(p, 2, Cooldown::TradingDays(2)),
            |p| p.loss_streak = None,
        ),
        (
            "stop.fixed_loss_pct",
            RiskField::FixedLossStop,
            |p| {
                p.stop.fixed_loss_pct = Some(PctRange {
                    low: d("2"),
                    high: d("3"),
                })
            },
            |p| {
                p.stop.fixed_loss_pct = Some(PctRange {
                    low: d("2"),
                    high: d("4"),
                })
            },
        ),
        (
            "stop.fixed_loss_pct",
            RiskField::FixedLossStop,
            |p| {
                p.stop.fixed_loss_pct = Some(PctRange {
                    low: d("2"),
                    high: d("3"),
                })
            },
            |p| {
                p.stop.fixed_loss_pct = Some(PctRange {
                    low: d("1.5"),
                    high: d("3"),
                })
            },
        ),
        (
            "stop.fixed_loss_pct",
            RiskField::FixedLossStop,
            |p| {
                p.stop.fixed_loss_pct = Some(PctRange {
                    low: d("2"),
                    high: d("3"),
                })
            },
            |p| p.stop.fixed_loss_pct = None,
        ),
        (
            "stop.atr",
            RiskField::AtrStop,
            |p| {
                p.stop.atr = Some(pi_template::AtrStop {
                    multiple: d("2"),
                    bar: pi_template::AtrBar::Minute,
                })
            },
            |p| {
                p.stop.atr = Some(pi_template::AtrStop {
                    multiple: d("3"),
                    bar: pi_template::AtrBar::Minute,
                })
            },
        ),
        (
            "stop.atr",
            RiskField::AtrStop,
            |p| {
                p.stop.atr = Some(pi_template::AtrStop {
                    multiple: d("2"),
                    bar: pi_template::AtrBar::Minute,
                })
            },
            |p| {
                p.stop.atr = Some(pi_template::AtrStop {
                    multiple: d("2"),
                    bar: pi_template::AtrBar::Day,
                })
            },
        ),
        (
            "stop.atr",
            RiskField::AtrStop,
            |p| {
                p.stop.atr = Some(pi_template::AtrStop {
                    multiple: d("2"),
                    bar: pi_template::AtrBar::Minute,
                })
            },
            |p| p.stop.atr = None,
        ),
        (
            "stop.thesis_break",
            RiskField::ThesisBreakStop,
            |p| p.stop.thesis_break = true,
            |p| p.stop.thesis_break = false,
        ),
        (
            "stop.flatten_before_close",
            RiskField::FlattenBeforeClose,
            |p| p.stop.flatten_before_close = true,
            |p| p.stop.flatten_before_close = false,
        ),
        (
            "max_holding",
            RiskField::MaxHolding,
            |p| p.max_holding = MaxHolding::TradingDays(5),
            |p| p.max_holding = MaxHolding::TradingDays(6),
        ),
        (
            "max_holding",
            RiskField::MaxHolding,
            |p| p.max_holding = MaxHolding::SameDay,
            |p| p.max_holding = MaxHolding::TradingDays(1),
        ),
        (
            "max_holding",
            RiskField::MaxHolding,
            |p| p.max_holding = MaxHolding::TradingDays(5),
            |p| p.max_holding = MaxHolding::Unlimited,
        ),
        (
            "chase_limit_pct",
            RiskField::ChaseLimit,
            |p| p.chase_limit_pct = Some(d("1")),
            |p| p.chase_limit_pct = Some(d("2")),
        ),
        (
            "chase_limit_pct",
            RiskField::ChaseLimit,
            |p| p.chase_limit_pct = Some(d("1")),
            |p| p.chase_limit_pct = None,
        ),
        (
            "promotion",
            RiskField::Promotion,
            |p| p.promotion.min_trades = 30,
            |p| p.promotion.min_trades = 20,
        ),
        (
            "promotion",
            RiskField::Promotion,
            |p| p.promotion.min_trading_days = 20,
            |p| p.promotion.min_trading_days = 10,
        ),
        (
            "promotion",
            RiskField::Promotion,
            |p| p.promotion.net_pnl_above = d("100"),
            |p| p.promotion.net_pnl_above = d("0"),
        ),
        (
            "promotion",
            RiskField::Promotion,
            |p| p.promotion.out_of_sample_pnl_above = Some(d("1")),
            |p| p.promotion.out_of_sample_pnl_above = Some(d("0")),
        ),
        (
            "promotion",
            RiskField::Promotion,
            |p| p.promotion.out_of_sample_pnl_above = Some(d("0")),
            |p| p.promotion.out_of_sample_pnl_above = None,
        ),
    ]
}

// 검증: P15, E28
#[test]
fn diff_marks_widening_in_both_directions_like_widened_fields() {
    for (i, (key, field, lo, hi)) in widening_cases().into_iter().enumerate() {
        let mut safe = base();
        lo(&mut safe);
        let mut risky = base();
        hi(&mut risky);

        // 안전 → 위험: 그 칸 하나만 바뀌고 넓힘.
        let up = diff_params(&safe, &risky);
        assert_eq!(up.len(), 1, "case {i} {key}: {up:?}");
        assert_eq!(up[0].key, key, "case {i}");
        assert!(up[0].widened, "case {i} {key}");
        assert_eq!(widened_fields(&safe, &risky), vec![field], "case {i} {key}");

        // 위험 → 안전: 바뀐 칸은 같고 넓힘이 아니다.
        let down = diff_params(&risky, &safe);
        assert_eq!(down.len(), 1, "case {i} {key}: {down:?}");
        assert_eq!(down[0].key, key, "case {i}");
        assert!(!down[0].widened, "case {i} {key}");
        assert!(widened_fields(&risky, &safe).is_empty(), "case {i} {key}");

        // 앞 · 뒤 값이 서로 맞바뀐다.
        assert_eq!(up[0].before, down[0].after, "case {i} {key}");
        assert_eq!(up[0].after, down[0].before, "case {i} {key}");
    }
}

// 검증: P15, E28
#[test]
fn diff_before_and_after_are_the_field_values() {
    let before = base();
    let mut after = before.clone();
    after.stop.atr = Some(pi_template::AtrStop {
        multiple: d("2"),
        bar: pi_template::AtrBar::Day,
    });
    let got = diff_params(&before, &after);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].key, "stop.atr");
    assert_eq!(got[0].before, val(&before.stop.atr));
    assert_eq!(got[0].after, val(&after.stop.atr));
}

// 검증: P15, E28
#[test]
fn diff_conviction_none_is_counted_as_the_symbol_weight() {
    // 확신 비중이 없던 것을 종목 비중보다 큰 값으로 → 넓힘, 같거나 작은 값으로 → 넓힘 아님.
    let mut before = base();
    before.conviction_weight_pct = None;
    before.symbol_weight_pct = d("10");
    let mut up = before.clone();
    up.conviction_weight_pct = Some(d("12"));
    let got = diff_params(&before, &up);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].key, "conviction_weight_pct");
    assert!(got[0].widened);
    assert_eq!(
        widened_fields(&before, &up),
        vec![RiskField::ConvictionWeight]
    );

    let mut same = before.clone();
    same.conviction_weight_pct = Some(d("10"));
    let got = diff_params(&before, &same);
    assert_eq!(got.len(), 1);
    assert!(!got[0].widened);
    assert!(widened_fields(&before, &same).is_empty());
}

// 검증: P15, E28
#[test]
fn diff_of_non_risk_fields_is_never_widened() {
    let before = base();
    let cases: Vec<(&str, Setter)> = vec![
        ("universe", |p| p.universe.min_market_cap = Some(d("1"))),
        ("trading_window", |p| {
            p.trading_window.skip_open_minutes += 5
        }),
        ("entry_signals", |p| {
            p.entry_signals = vec![pi_template::EntrySignal::Event]
        }),
        ("triggers", |p| {
            p.triggers = vec![pi_template::TriggerKind::Schedule]
        }),
        ("rebalance", |p| {
            p.rebalance = Some(pi_template::Rebalance {
                every_trading_days: 7,
                drift_pct: d("3"),
            })
        }),
        ("take_profit", |p| p.take_profit.trailing = true),
        ("approval_wait", |p| {
            p.approval_wait.buy = Duration::from_secs(1)
        }),
        ("unfilled_cancel", |p| {
            p.unfilled_cancel = UnfilledCancel::AtClose
        }),
    ];
    for (key, change) in cases {
        let mut after = before.clone();
        change(&mut after);
        let got = diff_params(&before, &after);
        let keys: Vec<&str> = got.iter().map(|c| c.key).collect();
        assert_eq!(keys, vec![key], "{key}");
        assert!(got.iter().all(|c| !c.widened), "{key}: {got:?}");
        assert!(widened_fields(&before, &after).is_empty(), "{key}");
    }
}

// 검증: P15, E28
#[test]
fn diff_between_every_pair_of_presets_agrees_with_widened_fields() {
    let key_of = |f: RiskField| -> &'static str {
        match f {
            RiskField::RiskPerTrade => "risk_per_trade_pct",
            RiskField::DailyLossLimit => "daily_loss_limit_pct",
            RiskField::MaxDrawdown => "max_drawdown_pct",
            RiskField::SymbolWeight => "symbol_weight_pct",
            RiskField::ConvictionWeight => "conviction_weight_pct",
            RiskField::SectorWeight => "sector_weight_pct",
            RiskField::AutoOrderLimit => "auto_order_limit_pct",
            RiskField::MinCash => "min_cash_pct",
            RiskField::MaxPositions => "max_positions",
            RiskField::MaxTradesPerDay => "max_trades_per_day",
            RiskField::LossStreak => "loss_streak",
            RiskField::FixedLossStop => "stop.fixed_loss_pct",
            RiskField::AtrStop => "stop.atr",
            RiskField::ThesisBreakStop => "stop.thesis_break",
            RiskField::FlattenBeforeClose => "stop.flatten_before_close",
            RiskField::MaxHolding => "max_holding",
            RiskField::ChaseLimit => "chase_limit_pct",
            RiskField::Promotion => "promotion",
            other => panic!("새 칸 {other:?}: 시험의 대응표를 고친다"),
        }
    };
    for a in Style::ALL {
        for b in Style::ALL {
            let (pa, pb) = (preset(a), preset(b));
            let diff = diff_params(&pa, &pb);
            if a == b {
                assert!(diff.is_empty(), "{a:?}");
                continue;
            }
            assert!(!diff.is_empty(), "{a:?} -> {b:?}");
            let widened_keys: Vec<&str> =
                diff.iter().filter(|c| c.widened).map(|c| c.key).collect();
            let want: Vec<&str> = widened_fields(&pa, &pb).into_iter().map(key_of).collect();
            assert_eq!(widened_keys, want, "{a:?} -> {b:?}");
            // 바뀐 칸은 보기 칸 순서를 따른다.
            let order: Vec<usize> = diff
                .iter()
                .map(|c| KEYS.iter().position(|k| *k == c.key).unwrap())
                .collect();
            let mut sorted = order.clone();
            sorted.sort();
            assert_eq!(order, sorted, "{a:?} -> {b:?}");
        }
    }
}

// ───────────────────────── 4. 실전 템플릿 멈추기의 전이 ─────────────────────────

// 검증: M3, V30
#[test]
fn live_to_stopped_by_user_is_allowed() {
    assert!(
        ALLOWED_TRANSITIONS
            .iter()
            .any(|t| t.from == TemplateState::Live
                && t.to == TemplateState::Stopped
                && t.reason == TransitionReason::UserStopped)
    );
    assert_eq!(
        TemplateState::Live.reasons_to(TemplateState::Stopped),
        vec![TransitionReason::UserStopped, TransitionReason::GlobalHalt]
    );
}

// 검증: M3, V30
#[test]
fn user_stop_is_only_from_live_and_other_edges_are_unchanged() {
    let user_stops: Vec<_> = ALLOWED_TRANSITIONS
        .iter()
        .filter(|t| t.reason == TransitionReason::UserStopped)
        .collect();
    assert_eq!(user_stops.len(), 1);
    // 다른 상태에서는 사용자 멈춤이 없다.
    for from in [
        TemplateState::Paper,
        TemplateState::PendingPromotion,
        TemplateState::Stopped,
    ] {
        assert!(
            !from
                .reasons_to(TemplateState::Stopped)
                .contains(&TransitionReason::UserStopped),
            "{from:?}"
        );
    }
    // 허용 표의 나머지 줄(5줄 중 4줄이 사용자 멈춤이 아닌 줄)은 그대로.
    let others: Vec<_> = ALLOWED_TRANSITIONS
        .iter()
        .filter(|t| t.reason != TransitionReason::UserStopped)
        .map(|t| (t.from, t.to, t.reason))
        .collect();
    assert_eq!(
        others,
        vec![
            (
                TemplateState::Paper,
                TemplateState::PendingPromotion,
                TransitionReason::CriteriaPassed
            ),
            (
                TemplateState::PendingPromotion,
                TemplateState::Live,
                TransitionReason::Approved
            ),
            (
                TemplateState::PendingPromotion,
                TemplateState::Paper,
                TransitionReason::Rejected
            ),
            (
                TemplateState::Live,
                TemplateState::Stopped,
                TransitionReason::GlobalHalt
            ),
        ]
    );
}
