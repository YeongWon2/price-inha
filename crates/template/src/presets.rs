//! 스타일별 기본값(C4 · V27).
//!
//! 출처: `docs/research/stock-investing-principles.md` 9장 표(모두 **추정 · 미확정**, 모의투자로
//! 검증한 뒤 확정한다), 승인 대기 시간 `dev.md` N2, 미체결 취소 시간 `dev.md` N4, 승격 기준
//! `docs/dev/decisions.md` 2026-11-16 30번.
//!
//! 9장 표에 값이 없는 칸(업종 비중 · 최소 현금 비중 · 유니버스 · 거래 시간대 · 리밸런싱)은
//! 보수적으로 고른 값이며 **추정 · 미확정 (출처 없음)** 이다. 칸마다 주석에 적는다.

use std::time::Duration;

use rust_decimal::Decimal;

use crate::params::{
    ApprovalWait, AtrBar, AtrStop, Cooldown, CountRange, EntrySignal, LossStreakStop, Market,
    MaxHolding, PctRange, Rebalance, RiskParams, StopRule, TakeProfitRule, TradingWindow,
    TriggerKind, UnfilledCancel, Universe,
};
use crate::promotion::PromotionCriteria;

/// 정수 퍼센트.
fn pct(n: i64) -> Decimal {
    Decimal::new(n, 0)
}

/// 소수 한 자리 퍼센트(`tenths` / 10).
fn pct_tenths(tenths: i64) -> Decimal {
    Decimal::new(tenths, 1)
}

/// 한 값 범위.
fn one(p: Decimal) -> PctRange {
    PctRange { low: p, high: p }
}

/// 승인 대기 시간(N2): 손절 1분 · 익절 5분 · 사는 주문 5분. 스타일에 관계없다.
fn approval_wait() -> ApprovalWait {
    ApprovalWait {
        stop_loss: Duration::from_secs(60),
        take_profit: Duration::from_secs(300),
        buy: Duration::from_secs(300),
    }
}

/// 승격 기준 기본값(결정 30번): 매매 30건 · 거래일 20일 · 비용 뺀 손익 > 0 ·
/// 최대 낙폭 ≤ 템플릿 한도의 절반 · 비용 뺀 표본 밖 손익 > 0(보수적으로 고름, 미확정).
fn promotion(max_drawdown_pct: Decimal) -> PromotionCriteria {
    PromotionCriteria {
        min_trades: 30,
        min_trading_days: 20,
        net_pnl_above: Decimal::ZERO,
        max_drawdown_pct: max_drawdown_pct / Decimal::TWO,
        out_of_sample_pnl_above: Some(Decimal::ZERO),
    }
}

/// 국내 시장만 보는 유니버스. 하한은 원화.
///
/// 추정 · 미확정 (출처 없음): 9장 표에 값이 없다. 하한 칸이 시장 통화 하나라서 1차는 국내만 둔다.
fn kr_universe(min_market_cap: Decimal, min_turnover: Decimal) -> Universe {
    Universe {
        markets: vec![Market::Kr],
        min_market_cap: Some(min_market_cap),
        min_turnover: Some(min_turnover),
        excluded_sectors: Vec::new(),
    }
}

/// 억 원.
fn eok(n: i64) -> Decimal {
    Decimal::new(n, 0) * Decimal::new(100_000_000, 0)
}

/// 단타(9장 표 단타 칸).
pub(crate) fn day_trade() -> RiskParams {
    let max_drawdown_pct = pct(10);
    RiskParams {
        risk_per_trade_pct: pct_tenths(5),
        daily_loss_limit_pct: pct_tenths(15),
        max_drawdown_pct,
        loss_streak: Some(LossStreakStop {
            losses: 3,
            cooldown: Cooldown::RestOfDay,
        }),
        max_positions: CountRange { low: 3, high: 3 },
        symbol_weight_pct: pct(20),
        conviction_weight_pct: None,
        // 추정 · 미확정 (출처 없음): 종목 20% 이상 · 상한 50% 안, 스타일이 짧아 가장 좁게.
        sector_weight_pct: pct(30),
        // 추정 · 미확정 (출처 없음).
        min_cash_pct: pct(10),
        stop: StopRule {
            fixed_loss_pct: Some(one(pct_tenths(15))),
            atr: Some(AtrStop {
                multiple: pct(1),
                bar: AtrBar::Minute,
            }),
            thesis_break: false,
            flatten_before_close: true,
        },
        take_profit: TakeProfitRule {
            r_target: Some(pct(2)),
            partial_at_target: false,
            trailing: true,
            valuation_exit: false,
            rebalance_exit: false,
        },
        max_holding: MaxHolding::SameDay,
        entry_signals: vec![EntrySignal::Breakout, EntrySignal::Event],
        // 추정 · 미확정 (출처 없음): 시가총액 1000억 원 · 거래대금 100억 원 이상.
        universe: kr_universe(eok(1_000), eok(100)),
        auto_order_limit_pct: pct(10),
        // 추정 · 미확정 (출처 없음): 9장 "장 시작 직후 n분"의 n = 10, 애프터마켓 쓰지 않음.
        trading_window: TradingWindow {
            skip_open_minutes: 10,
            after_market: false,
        },
        chase_limit_pct: Some(pct_tenths(5)),
        max_trades_per_day: 6,
        triggers: vec![
            TriggerKind::Quote,
            TriggerKind::Disclosure,
            TriggerKind::News,
        ],
        rebalance: None,
        approval_wait: approval_wait(),
        unfilled_cancel: UnfilledCancel::After(Duration::from_secs(60)),
        promotion: promotion(max_drawdown_pct),
    }
}

/// 스윙(9장 표 스윙 칸).
pub(crate) fn swing() -> RiskParams {
    let max_drawdown_pct = pct(15);
    RiskParams {
        risk_per_trade_pct: pct(1),
        daily_loss_limit_pct: pct(3),
        max_drawdown_pct,
        loss_streak: Some(LossStreakStop {
            losses: 4,
            cooldown: Cooldown::TradingDays(2),
        }),
        max_positions: CountRange { low: 5, high: 8 },
        symbol_weight_pct: pct(15),
        conviction_weight_pct: None,
        // 추정 · 미확정 (출처 없음): 종목 15% 이상 · 상한 50% 안, 단타보다 넉넉하게.
        sector_weight_pct: pct(35),
        // 추정 · 미확정 (출처 없음).
        min_cash_pct: pct(10),
        stop: StopRule {
            fixed_loss_pct: Some(PctRange {
                low: pct(7),
                high: pct(8),
            }),
            atr: Some(AtrStop {
                multiple: pct(2),
                bar: AtrBar::Day,
            }),
            thesis_break: false,
            flatten_before_close: false,
        },
        take_profit: TakeProfitRule {
            r_target: Some(pct(2)),
            partial_at_target: true,
            trailing: true,
            valuation_exit: false,
            rebalance_exit: false,
        },
        max_holding: MaxHolding::TradingDays(20),
        entry_signals: vec![
            EntrySignal::Breakout,
            EntrySignal::Pullback,
            EntrySignal::Event,
        ],
        // 추정 · 미확정 (출처 없음): 시가총액 3000억 원 · 거래대금 30억 원 이상.
        universe: kr_universe(eok(3_000), eok(30)),
        auto_order_limit_pct: pct(15),
        // 추정 · 미확정 (출처 없음): 장 시작 직후 10분 제외, 애프터마켓 쓰지 않음.
        trading_window: TradingWindow {
            skip_open_minutes: 10,
            after_market: false,
        },
        chase_limit_pct: Some(pct(2)),
        max_trades_per_day: 3,
        triggers: vec![TriggerKind::Quote, TriggerKind::Disclosure],
        rebalance: None,
        approval_wait: approval_wait(),
        unfilled_cancel: UnfilledCancel::After(Duration::from_secs(600)),
        promotion: promotion(max_drawdown_pct),
    }
}

/// 중장기(9장 표 중장기 칸).
pub(crate) fn long_term() -> RiskParams {
    let max_drawdown_pct = pct(20);
    RiskParams {
        risk_per_trade_pct: pct(1),
        daily_loss_limit_pct: pct(3),
        max_drawdown_pct,
        loss_streak: None,
        max_positions: CountRange { low: 10, high: 20 },
        symbol_weight_pct: pct(10),
        conviction_weight_pct: Some(pct(20)),
        // 추정 · 미확정 (출처 없음): 확신 종목 20% 이상 · 상한 50% 안, 가장 넉넉하게.
        sector_weight_pct: pct(40),
        // 추정 · 미확정 (출처 없음).
        min_cash_pct: pct(10),
        stop: StopRule {
            fixed_loss_pct: Some(one(pct(25))),
            atr: None,
            thesis_break: true,
            flatten_before_close: false,
        },
        take_profit: TakeProfitRule {
            r_target: None,
            partial_at_target: false,
            trailing: false,
            valuation_exit: true,
            rebalance_exit: true,
        },
        max_holding: MaxHolding::Unlimited,
        entry_signals: vec![EntrySignal::ValueRange],
        // 추정 · 미확정 (출처 없음): 시가총액 1조 원 · 거래대금 10억 원 이상.
        universe: kr_universe(eok(10_000), eok(10)),
        auto_order_limit_pct: pct(10),
        // 추정 · 미확정 (출처 없음): 장 시작 직후 30분 제외, 애프터마켓 쓰지 않음.
        trading_window: TradingWindow {
            skip_open_minutes: 30,
            after_market: false,
        },
        chase_limit_pct: None,
        max_trades_per_day: 1,
        triggers: vec![TriggerKind::Disclosure, TriggerKind::Schedule],
        // 추정 · 미확정 (출처 없음): 분기(63영업일)마다, 목표 비중에서 5%p 넘게 벗어나면.
        rebalance: Some(Rebalance {
            every_trading_days: 63,
            drift_pct: pct(5),
        }),
        approval_wait: approval_wait(),
        unfilled_cancel: UnfilledCancel::AtClose,
        promotion: promotion(max_drawdown_pct),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::check;

    #[test]
    fn every_preset_passes_the_full_cap_check_and_sector_covers_symbol() {
        for p in [day_trade(), swing(), long_term()] {
            assert!(check(&p).is_ok());
            assert!(p.sector_weight_pct >= p.symbol_weight_pct);
            if let Some(w) = p.conviction_weight_pct {
                assert!(p.sector_weight_pct >= w);
            }
        }
    }
}
