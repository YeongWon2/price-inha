//! 스타일 프리셋(C4 · V27 · N2 · N4 · M3 · P14).
//!
//! 기대값 출처: `docs/research/stock-investing-principles.md` 9장 표, `dev.md` N2 · N4,
//! `docs/dev/decisions.md` 2026-11-16 30번. 표와 끝자리까지 비교한다.

use crate::common::d;
use std::time::Duration;

use pi_template::{
    AbsoluteCaps, AtrBar, AtrStop, Cooldown, CountRange, Criterion, EntrySignal, LossStreakStop,
    MaxHolding, PctRange, PromotionStats, Style, TriggerKind, UnfilledCancel, Verdict, preset,
};
use rust_decimal::Decimal;

// 검증: C4, V27
#[test]
fn three_styles_can_be_chosen_with_korean_labels() {
    assert_eq!(Style::ALL, [Style::DayTrade, Style::Swing, Style::LongTerm]);
    assert_eq!(Style::DayTrade.label(), "단타");
    assert_eq!(Style::Swing.label(), "스윙");
    assert_eq!(Style::LongTerm.label(), "중장기");
}

// 검증: C4, V27
#[test]
fn day_trade_preset_matches_research_table() {
    let p = preset(Style::DayTrade);
    assert_eq!(p.risk_per_trade_pct, d("0.5"));
    assert_eq!(p.daily_loss_limit_pct, d("1.5"));
    assert_eq!(p.max_drawdown_pct, d("10"));
    assert_eq!(
        p.loss_streak,
        Some(LossStreakStop {
            losses: 3,
            cooldown: Cooldown::RestOfDay
        })
    );
    assert_eq!(p.max_positions, CountRange { low: 3, high: 3 });
    assert_eq!(p.symbol_weight_pct, d("20"));
    assert_eq!(p.conviction_weight_pct, None);
    assert_eq!(
        p.stop.fixed_loss_pct,
        Some(PctRange {
            low: d("1.5"),
            high: d("1.5")
        })
    );
    assert_eq!(
        p.stop.atr,
        Some(AtrStop {
            multiple: d("1"),
            bar: AtrBar::Minute
        })
    );
    assert!(!p.stop.thesis_break);
    assert!(p.stop.flatten_before_close);
    assert_eq!(p.take_profit.r_target, Some(d("2")));
    assert!(!p.take_profit.partial_at_target);
    assert!(p.take_profit.trailing);
    assert!(!p.take_profit.valuation_exit);
    assert!(!p.take_profit.rebalance_exit);
    assert_eq!(p.max_holding, MaxHolding::SameDay);
    assert!(p.entry_signals.contains(&EntrySignal::Breakout));
    assert!(p.entry_signals.contains(&EntrySignal::Event));
    assert_eq!(p.auto_order_limit_pct, d("10"));
    assert_eq!(p.max_trades_per_day, 6);
    assert_eq!(p.chase_limit_pct, Some(d("0.5")));
    assert!(p.triggers.contains(&TriggerKind::Quote));
    assert!(p.triggers.contains(&TriggerKind::Disclosure));
    assert!(p.triggers.contains(&TriggerKind::News));
    assert_eq!(p.rebalance, None);
}

// 검증: C4, V27
#[test]
fn swing_preset_matches_research_table() {
    let p = preset(Style::Swing);
    assert_eq!(p.risk_per_trade_pct, d("1"));
    assert_eq!(p.daily_loss_limit_pct, d("3"));
    assert_eq!(p.max_drawdown_pct, d("15"));
    assert_eq!(
        p.loss_streak,
        Some(LossStreakStop {
            losses: 4,
            cooldown: Cooldown::TradingDays(2)
        })
    );
    assert_eq!(p.max_positions, CountRange { low: 5, high: 8 });
    assert_eq!(p.symbol_weight_pct, d("15"));
    assert_eq!(p.conviction_weight_pct, None);
    assert_eq!(
        p.stop.fixed_loss_pct,
        Some(PctRange {
            low: d("7"),
            high: d("8")
        })
    );
    assert_eq!(
        p.stop.atr,
        Some(AtrStop {
            multiple: d("2"),
            bar: AtrBar::Day
        })
    );
    assert!(!p.stop.thesis_break);
    assert!(!p.stop.flatten_before_close);
    assert_eq!(p.take_profit.r_target, Some(d("2")));
    assert!(p.take_profit.partial_at_target);
    assert!(p.take_profit.trailing);
    assert!(!p.take_profit.valuation_exit);
    assert!(!p.take_profit.rebalance_exit);
    assert_eq!(p.max_holding, MaxHolding::TradingDays(20));
    assert!(p.entry_signals.contains(&EntrySignal::Breakout));
    assert!(p.entry_signals.contains(&EntrySignal::Pullback));
    assert!(p.entry_signals.contains(&EntrySignal::Event));
    assert_eq!(p.auto_order_limit_pct, d("15"));
    assert_eq!(p.max_trades_per_day, 3);
    assert_eq!(p.chase_limit_pct, Some(d("2")));
    assert!(p.triggers.contains(&TriggerKind::Disclosure));
    assert_eq!(p.rebalance, None);
}

// 검증: C4, V27
#[test]
fn long_term_preset_matches_research_table() {
    let p = preset(Style::LongTerm);
    assert_eq!(p.risk_per_trade_pct, d("1"));
    assert_eq!(p.daily_loss_limit_pct, d("3"));
    assert_eq!(p.max_drawdown_pct, d("20"));
    assert_eq!(p.loss_streak, None);
    assert_eq!(p.max_positions, CountRange { low: 10, high: 20 });
    assert_eq!(p.symbol_weight_pct, d("10"));
    assert_eq!(p.conviction_weight_pct, Some(d("20")));
    assert_eq!(
        p.stop.fixed_loss_pct,
        Some(PctRange {
            low: d("25"),
            high: d("25")
        })
    );
    assert_eq!(p.stop.atr, None);
    assert!(p.stop.thesis_break);
    assert!(!p.stop.flatten_before_close);
    assert_eq!(p.take_profit.r_target, None);
    assert!(!p.take_profit.trailing);
    assert!(p.take_profit.valuation_exit);
    assert!(p.take_profit.rebalance_exit);
    assert_eq!(p.max_holding, MaxHolding::Unlimited);
    assert!(p.entry_signals.contains(&EntrySignal::ValueRange));
    assert_eq!(p.auto_order_limit_pct, d("10"));
    assert_eq!(p.max_trades_per_day, 1);
    assert_eq!(p.chase_limit_pct, None);
    assert!(p.triggers.contains(&TriggerKind::Disclosure));
    assert!(p.triggers.contains(&TriggerKind::Schedule));
    assert!(p.rebalance.is_some());
}

// 검증: N2
#[test]
fn approval_wait_defaults_are_one_five_five_minutes_for_every_style() {
    for style in Style::ALL {
        let w = preset(style).approval_wait;
        assert_eq!(w.stop_loss, Duration::from_secs(60), "{style:?}");
        assert_eq!(w.take_profit, Duration::from_secs(300), "{style:?}");
        assert_eq!(w.buy, Duration::from_secs(300), "{style:?}");
    }
}

// 검증: N4
#[test]
fn unfilled_cancel_defaults_follow_style() {
    assert_eq!(
        preset(Style::DayTrade).unfilled_cancel,
        UnfilledCancel::After(Duration::from_secs(60))
    );
    assert_eq!(
        preset(Style::Swing).unfilled_cancel,
        UnfilledCancel::After(Duration::from_secs(600))
    );
    assert_eq!(
        preset(Style::LongTerm).unfilled_cancel,
        UnfilledCancel::AtClose
    );
}

// 검증: M3, P14
#[test]
fn promotion_defaults_are_30_trades_20_days_positive_pnl_and_half_drawdown() {
    let half = [
        (Style::DayTrade, d("5")),
        (Style::Swing, d("7.5")),
        (Style::LongTerm, d("10")),
    ];
    for (style, expected_mdd) in half {
        let p = preset(style);
        let c = &p.promotion;
        assert_eq!(c.min_trades, 30, "{style:?}");
        assert_eq!(c.min_trading_days, 20, "{style:?}");
        assert_eq!(c.net_pnl_above, Decimal::ZERO, "{style:?}");
        assert_eq!(c.max_drawdown_pct, expected_mdd, "{style:?}");
        assert_eq!(c.max_drawdown_pct * d("2"), p.max_drawdown_pct, "{style:?}");
        assert_eq!(c.out_of_sample_pnl_above, Some(Decimal::ZERO), "{style:?}");
    }
}

// 검증: M3, P14
#[test]
fn preset_promotion_criteria_judge_a_typical_result() {
    let c = preset(Style::Swing).promotion;
    let good = PromotionStats {
        trades: 30,
        trading_days: 20,
        net_pnl: d("0.01"),
        max_drawdown_pct: d("7.5"),
        out_of_sample_pnl: Some(d("1")),
    };
    assert_eq!(c.evaluate(&good), Verdict::Passed);
    let thin = PromotionStats { trades: 29, ..good };
    assert_eq!(c.evaluate(&thin), Verdict::Short(vec![Criterion::Trades]));
}

// 검증: C4, M3
#[test]
fn every_preset_is_inside_absolute_caps() {
    let caps = AbsoluteCaps::FIXED;
    for style in Style::ALL {
        let p = preset(style);
        assert!(p.risk_per_trade_pct > Decimal::ZERO);
        assert!(p.risk_per_trade_pct <= caps.risk_per_trade_pct, "{style:?}");
        assert!(p.daily_loss_limit_pct <= caps.daily_loss_pct, "{style:?}");
        assert!(p.max_drawdown_pct <= caps.max_drawdown_pct, "{style:?}");
        assert!(p.symbol_weight_pct <= caps.symbol_weight_pct, "{style:?}");
        if let Some(w) = p.conviction_weight_pct {
            assert!(w <= caps.symbol_weight_pct, "{style:?}");
        }
        assert!(p.sector_weight_pct <= caps.sector_weight_pct, "{style:?}");
        assert!(p.sector_weight_pct >= p.symbol_weight_pct, "{style:?}");
        assert!(p.min_cash_pct >= Decimal::ZERO, "{style:?}");
        assert!(p.min_cash_pct < d("100"), "{style:?}");
    }
}

// 검증: C4
#[test]
fn preset_is_a_fresh_copy_each_time() {
    // 원본(Hermes) 프로필 배포: 묶음을 통째로 고르고 사본 위에서 바꾼다. 사본을 바꿔도 프리셋은 그대로다.
    let mut copy = preset(Style::DayTrade);
    copy.risk_per_trade_pct = d("0.1");
    copy.max_trades_per_day = 1;
    let again = preset(Style::DayTrade);
    assert_eq!(again.risk_per_trade_pct, d("0.5"));
    assert_eq!(again.max_trades_per_day, 6);
    assert_eq!(preset(Style::Swing), preset(Style::Swing));
    assert_ne!(preset(Style::DayTrade), preset(Style::Swing));
}
